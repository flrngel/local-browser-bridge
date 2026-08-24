import AppKit
import ApplicationServices
import CoreGraphics
import Darwin
import Foundation

private typealias MainConnectionFunction = @convention(c) () -> UInt32
private typealias ActiveSpaceFunction = @convention(c) (UInt32) -> UInt64
private typealias GetFrontProcessFunction = @convention(c) (UnsafeMutableRawPointer) -> Int32
private typealias GetProcessPIDFunction = @convention(c) (UnsafeRawPointer, UnsafeMutablePointer<pid_t>) -> Int32

@_silgen_name("_AXUIElementGetWindow")
private func axWindowIdentifier(
    _ element: AXUIElement,
    _ identifier: UnsafeMutablePointer<CGWindowID>
) -> AXError

private func loadFunction<T>(_ handle: UnsafeMutableRawPointer?, _ name: String, as type: T.Type) -> T? {
    guard let symbol = dlsym(handle, name) else { return nil }
    return unsafeBitCast(symbol, to: type)
}

private func activeSpaceIdentifier() -> UInt64 {
    let framework = "/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight"
    guard let handle = dlopen(framework, RTLD_LAZY | RTLD_LOCAL) else { return 0 }
    defer { dlclose(handle) }

    guard
        let mainConnection = loadFunction(handle, "CGSMainConnectionID", as: MainConnectionFunction.self),
        let activeSpace = loadFunction(handle, "SLSGetActiveSpace", as: ActiveSpaceFunction.self)
            ?? loadFunction(handle, "CGSGetActiveSpace", as: ActiveSpaceFunction.self)
    else { return 0 }

    return activeSpace(mainConnection())
}

private struct RawFrontProcessIdentity {
    let processSerialNumber: [UInt8]
    let pid: pid_t
}

private func rawFrontProcessIdentity() -> RawFrontProcessIdentity? {
    let skyLightPath = "/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight"
    let applicationServicesPath = "/System/Library/Frameworks/ApplicationServices.framework/ApplicationServices"
    guard
        let skyLight = dlopen(skyLightPath, RTLD_LAZY | RTLD_LOCAL),
        let applicationServices = dlopen(applicationServicesPath, RTLD_LAZY | RTLD_LOCAL)
    else { return nil }
    defer {
        dlclose(applicationServices)
        dlclose(skyLight)
    }

    guard
        let getFrontProcess = loadFunction(skyLight, "_SLPSGetFrontProcess", as: GetFrontProcessFunction.self),
        let getProcessPID = loadFunction(applicationServices, "GetProcessPID", as: GetProcessPIDFunction.self)
    else { return nil }

    var processSerialNumber = [UInt8](repeating: 0, count: 8)
    let frontStatus = processSerialNumber.withUnsafeMutableBytes { buffer in
        getFrontProcess(buffer.baseAddress!)
    }
    guard frontStatus == 0 else { return nil }
    var pid: pid_t = 0
    let pidStatus = processSerialNumber.withUnsafeBytes { buffer in
        getProcessPID(buffer.baseAddress!, &pid)
    }
    guard pidStatus == 0, pid > 0 else { return nil }
    return RawFrontProcessIdentity(processSerialNumber: processSerialNumber, pid: pid)
}

private func processSerialNumberHex(_ value: [UInt8]) -> String {
    value.map { String(format: "%02x", $0) }.joined()
}

private func frontWindowIdentifier(for pid: pid_t) -> UInt32 {
    guard
        pid > 0,
        let rawWindows = CGWindowListCopyWindowInfo(
            [.optionOnScreenOnly, .excludeDesktopElements],
            kCGNullWindowID
        ) as? [[String: Any]]
    else { return 0 }

    for window in rawWindows {
        let ownerPID = (window[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value ?? 0
        let layer = (window[kCGWindowLayer as String] as? NSNumber)?.intValue ?? -1
        let alpha = (window[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0
        guard ownerPID == pid, layer == 0, alpha > 0 else { continue }
        return (window[kCGWindowNumber as String] as? NSNumber)?.uint32Value ?? 0
    }
    return 0
}

private enum PointerPromptState: String {
    case waiting = "WAITING"
    case move = "MOVE"
    case action = "ACTION"
    case complete = "COMPLETE"

    var title: String {
        switch self {
        case .waiting: "LBB macOS Acceptance - WAITING"
        case .move: "LBB macOS Acceptance - MOVE POINTER"
        case .action: "LBB macOS Acceptance - ACTION RUNNING"
        case .complete: "LBB macOS Acceptance - COMPLETE"
        }
    }
}

private struct PointerPromptObservation {
    let requested: Bool
    let ownerMatched: Bool
    let titleMatched: Bool
    let onScreen: Bool
    let nonactivating: Bool
}

private func pointerPromptObservation(
    for promptPID: pid_t,
    expectedState: PointerPromptState?,
    foregroundPID: pid_t
) -> PointerPromptObservation {
    guard promptPID > 0, let expectedState else {
        return PointerPromptObservation(
            requested: false,
            ownerMatched: false,
            titleMatched: false,
            onScreen: false,
            nonactivating: false
        )
    }
    guard
        let rawWindows = CGWindowListCopyWindowInfo(
            [.optionOnScreenOnly, .excludeDesktopElements],
            kCGNullWindowID
        ) as? [[String: Any]]
    else {
        return PointerPromptObservation(
            requested: true,
            ownerMatched: false,
            titleMatched: false,
            onScreen: false,
            nonactivating: false
        )
    }

    let ownedWindows = rawWindows.filter { window in
        (window[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == promptPID
    }
    let exactWindows = ownedWindows.filter { window in
        let title = window[kCGWindowName as String] as? String
        let alpha = (window[kCGWindowAlpha as String] as? NSNumber)?.doubleValue ?? 0
        let bounds = window[kCGWindowBounds as String] as? [String: Any]
        let width = (bounds?["Width"] as? NSNumber)?.doubleValue ?? 0
        let height = (bounds?["Height"] as? NSNumber)?.doubleValue ?? 0
        return title == expectedState.title && alpha > 0 && width >= 1 && height >= 1
    }
    let exact = exactWindows.count == 1 && ownedWindows.count == 1
    return PointerPromptObservation(
        requested: true,
        ownerMatched: exact,
        titleMatched: exact,
        onScreen: exact,
        nonactivating: exact && foregroundPID > 0 && foregroundPID != promptPID &&
            frontmostAttribute(for: promptPID) == false
    )
}

private func windowIdentifier(for pid: pid_t, attribute: CFString) -> UInt32 {
    guard pid > 0 else { return 0 }
    let application = AXUIElementCreateApplication(pid)
    var rawWindow: CFTypeRef?
    guard
        AXUIElementCopyAttributeValue(
            application,
            attribute,
            &rawWindow
        ) == .success,
        let rawWindow
    else { return 0 }

    let window = unsafeBitCast(rawWindow, to: AXUIElement.self)
    var identifier: CGWindowID = 0
    return axWindowIdentifier(window, &identifier) == .success ? identifier : 0
}

private func focusedWindowIdentifier(for pid: pid_t) -> UInt32 {
    windowIdentifier(for: pid, attribute: kAXFocusedWindowAttribute as CFString)
}

private func mainWindowIdentifier(for pid: pid_t) -> UInt32 {
    windowIdentifier(for: pid, attribute: kAXMainWindowAttribute as CFString)
}

private func frontmostAttribute(for pid: pid_t) -> Bool? {
    guard pid > 0 else { return nil }
    let application = AXUIElementCreateApplication(pid)
    var rawFrontmost: CFTypeRef?
    guard
        AXUIElementCopyAttributeValue(
            application,
            kAXFrontmostAttribute as CFString,
            &rawFrontmost
        ) == .success,
        let rawFrontmost
    else { return nil }
    return rawFrontmost as? Bool
}

private let hidPointerCounterTypes: [(String, CGEventType)] = [
    ("leftMouseDown", .leftMouseDown),
    ("leftMouseUp", .leftMouseUp),
    ("rightMouseDown", .rightMouseDown),
    ("rightMouseUp", .rightMouseUp),
    ("mouseMoved", .mouseMoved),
    ("leftMouseDragged", .leftMouseDragged),
    ("rightMouseDragged", .rightMouseDragged),
    ("scrollWheel", .scrollWheel),
    ("otherMouseDragged", .otherMouseDragged),
    ("otherMouseDown", .otherMouseDown),
    ("otherMouseUp", .otherMouseUp),
    ("tabletPointer", .tabletPointer),
    ("tabletProximity", .tabletProximity),
]
private let maximumHidPointerCounterAdvance: UInt32 = 1_000_000

private enum HidPointerCounterProgress {
    case stable
    case advanced
    case unknown
}

private struct PointerSample {
    let location: CGPoint
    let hidPointerCounters: [String: UInt32]
    let boundaryActivityObserved: Bool
    let monitorHealthy: Bool
}

private func hidPointerCounters() -> [String: UInt32] {
    Dictionary(uniqueKeysWithValues: hidPointerCounterTypes.map { entry in
        let (name, eventType) = entry
        return (
            name,
            CGEventSource.counterForEventType(.hidSystemState, eventType: eventType)
        )
    })
}

private func hidPointerCounterProgress(
    from before: [String: UInt32],
    to after: [String: UInt32]
) -> HidPointerCounterProgress {
    var advanced = false
    for (name, _) in hidPointerCounterTypes {
        guard let beforeValue = before[name], let afterValue = after[name] else {
            return .unknown
        }
        let delta = afterValue &- beforeValue
        if delta > maximumHidPointerCounterAdvance { return .unknown }
        if delta > 0 { advanced = true }
    }
    return advanced ? .advanced : .stable
}

private func pointerSample() -> PointerSample {
    let deadline = DispatchTime.now().uptimeNanoseconds + 30_000_000
    var boundaryActivityObserved = false
    while true {
        let before = hidPointerCounters()
        guard
            let source = CGEventSource(stateID: .hidSystemState),
            let location = CGEvent(source: source)?.location
        else {
            return PointerSample(
                location: .zero,
                hidPointerCounters: before,
                boundaryActivityObserved: boundaryActivityObserved,
                monitorHealthy: false
            )
        }
        let after = hidPointerCounters()
        switch hidPointerCounterProgress(from: before, to: after) {
        case .stable:
            return PointerSample(
                location: location,
                hidPointerCounters: after,
                boundaryActivityObserved: boundaryActivityObserved,
                monitorHealthy: true
            )
        case .advanced:
            boundaryActivityObserved = true
            if DispatchTime.now().uptimeNanoseconds >= deadline {
                return PointerSample(
                    location: location,
                    hidPointerCounters: after,
                    boundaryActivityObserved: true,
                    monitorHealthy: true
                )
            }
        case .unknown:
            return PointerSample(
                location: location,
                hidPointerCounters: after,
                boundaryActivityObserved: boundaryActivityObserved,
                monitorHealthy: false
            )
        }
        usleep(1_000)
    }
}

let arguments = Array(CommandLine.arguments.dropFirst())
let targetPID = arguments.first.flatMap(Int32.init) ?? 0
let expectedActiveTargetWindowID = arguments.dropFirst().first.flatMap(UInt32.init) ?? 0
let requestedWaitMilliseconds = arguments.dropFirst(2).first.flatMap(UInt64.init) ?? 0
let pointerPromptPID = arguments.dropFirst(3).first.flatMap(Int32.init) ?? 0
private let pointerPromptState = arguments.dropFirst(4).first.flatMap(PointerPromptState.init(rawValue:))
let waitMilliseconds = min(requestedWaitMilliseconds, 10_000)
var activeTargetObserved = false
var observedTargetFocusedWindowID: UInt32 = 0
var observedTargetMainWindowID: UInt32 = 0
if targetPID > 0, expectedActiveTargetWindowID > 0, waitMilliseconds > 0 {
    let deadline = DispatchTime.now().uptimeNanoseconds + waitMilliseconds * 1_000_000
    repeat {
        let focused = focusedWindowIdentifier(for: targetPID)
        let main = mainWindowIdentifier(for: targetPID)
        if focused == expectedActiveTargetWindowID && main == expectedActiveTargetWindowID &&
            frontmostAttribute(for: targetPID) == true
        {
            activeTargetObserved = true
            observedTargetFocusedWindowID = focused
            observedTargetMainWindowID = main
            break
        }
        usleep(2_000)
    } while DispatchTime.now().uptimeNanoseconds < deadline
}

private let pointerBeforePrompt = pointerSample()
private let rawForegroundBefore = rawFrontProcessIdentity()
let foregroundPIDBefore = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0
let foregroundAXFocusedWindowID = focusedWindowIdentifier(for: foregroundPIDBefore)
let foregroundAXMainWindowID = mainWindowIdentifier(for: foregroundPIDBefore)
let foregroundAXFrontmostValue = frontmostAttribute(for: foregroundPIDBefore)
let foregroundAXFrontmost = foregroundAXFrontmostValue ?? false
let foregroundPIDAfter = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0
private let rawForegroundAfter = rawFrontProcessIdentity()
let foregroundProbeHealthy = foregroundPIDBefore > 0 && foregroundPIDAfter > 0 &&
    rawForegroundBefore != nil && rawForegroundAfter != nil &&
    rawForegroundBefore!.pid > 0 && rawForegroundAfter!.pid > 0
let foregroundIdentityStable = foregroundPIDBefore > 0 && foregroundPIDBefore == foregroundPIDAfter
let rawForegroundIdentityStable = rawForegroundBefore != nil && rawForegroundAfter != nil &&
    rawForegroundBefore!.pid == rawForegroundAfter!.pid &&
    rawForegroundBefore!.processSerialNumber == rawForegroundAfter!.processSerialNumber &&
    rawForegroundBefore!.pid == foregroundPIDBefore && foregroundPIDBefore == foregroundPIDAfter
let foregroundTransitionObserved = foregroundProbeHealthy &&
    (!foregroundIdentityStable || !rawForegroundIdentityStable)
let foregroundAXProbeHealthy = foregroundIdentityStable &&
    foregroundAXFocusedWindowID > 0 && foregroundAXMainWindowID > 0 &&
    foregroundAXFrontmostValue != nil
let foregroundPID = foregroundIdentityStable ? foregroundPIDBefore : 0
private let pointerPrompt = pointerPromptObservation(
    for: pointerPromptPID,
    expectedState: pointerPromptState,
    foregroundPID: foregroundPID
)
// A prompt-bound probe returns a second HID sample taken only after the exact
// prompt state was observed. Comparing its cumulative counters with the prior
// probe therefore covers activity through the visible state transition.
private let pointer = pointerPrompt.requested ? pointerSample() : pointerBeforePrompt
let pointerBoundaryActivityObserved = pointerBeforePrompt.boundaryActivityObserved ||
    pointer.boundaryActivityObserved
let pointerActivityMonitorHealthy = pointerBeforePrompt.monitorHealthy && pointer.monitorHealthy
let targetFocusedWindowID = activeTargetObserved
    ? observedTargetFocusedWindowID
    : focusedWindowIdentifier(for: targetPID)
let targetMainWindowID = activeTargetObserved
    ? observedTargetMainWindowID
    : mainWindowIdentifier(for: targetPID)
let activeSpace = activeSpaceIdentifier()
let probe: [String: Any] = [
    "accessibilityReady": AXIsProcessTrusted(),
    "activeSpace": activeSpace,
    "activeSpaceProbeHealthy": activeSpace > 0,
    // Coordinates and raw counters are consumed ephemerally by the rig.
    // Only bounded equality/activity/health booleans and state enums are retained.
    "cursorX": pointer.location.x,
    "cursorY": pointer.location.y,
    "hidPointerCounters": pointer.hidPointerCounters,
    "pointerBoundaryActivityObserved": pointerBoundaryActivityObserved,
    "pointerActivityMonitorHealthy": pointerActivityMonitorHealthy,
    "foregroundPID": foregroundPID,
    "foregroundProbeHealthy": foregroundProbeHealthy,
    "foregroundTransitionObserved": foregroundTransitionObserved,
    "foregroundIdentityStable": foregroundIdentityStable,
    "foregroundAXFocusedWindowID": foregroundIdentityStable ? foregroundAXFocusedWindowID : 0,
    "foregroundAXMainWindowID": foregroundIdentityStable ? foregroundAXMainWindowID : 0,
    "foregroundAXFrontmost": foregroundIdentityStable && foregroundAXFrontmost,
    "foregroundAXProbeHealthy": foregroundAXProbeHealthy,
    "frontWindowID": frontWindowIdentifier(for: foregroundPID),
    "rawForegroundPID": rawForegroundIdentityStable ? rawForegroundBefore!.pid : 0,
    "rawForegroundPSN": rawForegroundIdentityStable
        ? processSerialNumberHex(rawForegroundBefore!.processSerialNumber)
        : "",
    "rawForegroundIdentityStable": rawForegroundIdentityStable,
    "screenCaptureReady": CGPreflightScreenCaptureAccess(),
    "targetFocusedWindowID": targetFocusedWindowID,
    "targetMainWindowID": targetMainWindowID,
    "targetAXFrontmost": activeTargetObserved || (frontmostAttribute(for: targetPID) ?? false),
    "activeTargetObserved": activeTargetObserved,
    // Prompt PID and title are consumed only inside this process. The runner
    // receives bounded delivery/topology booleans, never those raw values.
    "pointerPromptProbeRequested": pointerPrompt.requested,
    "pointerPromptOwnerMatched": pointerPrompt.ownerMatched,
    "pointerPromptTitleMatched": pointerPrompt.titleMatched,
    "pointerPromptOnScreen": pointerPrompt.onScreen,
    "pointerPromptNonactivating": pointerPrompt.nonactivating,
]
let data = try JSONSerialization.data(withJSONObject: probe, options: [.sortedKeys])
print(String(decoding: data, as: UTF8.self))
