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

private let pointer = pointerSample()
private let rawForegroundBefore = rawFrontProcessIdentity()
let foregroundPIDBefore = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0
let foregroundAXFocusedWindowID = focusedWindowIdentifier(for: foregroundPIDBefore)
let foregroundAXMainWindowID = mainWindowIdentifier(for: foregroundPIDBefore)
let foregroundAXFrontmost = frontmostAttribute(for: foregroundPIDBefore) ?? false
let foregroundPIDAfter = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0
private let rawForegroundAfter = rawFrontProcessIdentity()
let foregroundIdentityStable = foregroundPIDBefore > 0 && foregroundPIDBefore == foregroundPIDAfter
let rawForegroundIdentityStable = rawForegroundBefore != nil && rawForegroundAfter != nil &&
    rawForegroundBefore!.pid == rawForegroundAfter!.pid &&
    rawForegroundBefore!.processSerialNumber == rawForegroundAfter!.processSerialNumber &&
    rawForegroundBefore!.pid == foregroundPIDBefore && foregroundPIDBefore == foregroundPIDAfter
let foregroundPID = foregroundIdentityStable ? foregroundPIDBefore : 0
let targetFocusedWindowID = activeTargetObserved
    ? observedTargetFocusedWindowID
    : focusedWindowIdentifier(for: targetPID)
let targetMainWindowID = activeTargetObserved
    ? observedTargetMainWindowID
    : mainWindowIdentifier(for: targetPID)
let probe: [String: Any] = [
    "accessibilityReady": AXIsProcessTrusted(),
    "activeSpace": activeSpaceIdentifier(),
    // Coordinates and raw counters are consumed ephemerally by the rig.
    // Only bounded equality/activity/health booleans and state enums are retained.
    "cursorX": pointer.location.x,
    "cursorY": pointer.location.y,
    "hidPointerCounters": pointer.hidPointerCounters,
    "pointerBoundaryActivityObserved": pointer.boundaryActivityObserved,
    "pointerActivityMonitorHealthy": pointer.monitorHealthy,
    "foregroundPID": foregroundPID,
    "foregroundIdentityStable": foregroundIdentityStable,
    "foregroundAXFocusedWindowID": foregroundIdentityStable ? foregroundAXFocusedWindowID : 0,
    "foregroundAXMainWindowID": foregroundIdentityStable ? foregroundAXMainWindowID : 0,
    "foregroundAXFrontmost": foregroundIdentityStable && foregroundAXFrontmost,
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
]
let data = try JSONSerialization.data(withJSONObject: probe, options: [.sortedKeys])
print(String(decoding: data, as: UTF8.self))
