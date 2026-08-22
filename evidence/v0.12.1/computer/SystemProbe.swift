import AppKit
import ApplicationServices
import CoreGraphics
import Darwin
import Foundation

private typealias MainConnectionFunction = @convention(c) () -> UInt32
private typealias ActiveSpaceFunction = @convention(c) (UInt32) -> UInt64

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

private func focusedWindowIdentifier(for pid: pid_t) -> UInt32 {
    guard pid > 0 else { return 0 }
    let application = AXUIElementCreateApplication(pid)
    var rawFocusedWindow: CFTypeRef?
    guard
        AXUIElementCopyAttributeValue(
            application,
            kAXFocusedWindowAttribute as CFString,
            &rawFocusedWindow
        ) == .success,
        let rawFocusedWindow
    else { return 0 }

    let focusedWindow = unsafeBitCast(rawFocusedWindow, to: AXUIElement.self)
    var identifier: CGWindowID = 0
    return axWindowIdentifier(focusedWindow, &identifier) == .success ? identifier : 0
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

let cursor = CGEvent(source: nil)?.location ?? .zero
let foregroundPIDBefore = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0
let foregroundAXFocusedWindowID = focusedWindowIdentifier(for: foregroundPIDBefore)
let foregroundAXFrontmost = frontmostAttribute(for: foregroundPIDBefore) ?? false
let foregroundPIDAfter = NSWorkspace.shared.frontmostApplication?.processIdentifier ?? 0
let foregroundIdentityStable = foregroundPIDBefore > 0 && foregroundPIDBefore == foregroundPIDAfter
let foregroundPID = foregroundIdentityStable ? foregroundPIDBefore : 0
let targetPID = CommandLine.arguments.dropFirst().first.flatMap(Int32.init) ?? 0
let probe: [String: Any] = [
    "accessibilityReady": AXIsProcessTrusted(),
    "activeSpace": activeSpaceIdentifier(),
    "cursorX": cursor.x,
    "cursorY": cursor.y,
    "foregroundPID": foregroundPID,
    "foregroundIdentityStable": foregroundIdentityStable,
    "foregroundAXFocusedWindowID": foregroundIdentityStable ? foregroundAXFocusedWindowID : 0,
    "foregroundAXFrontmost": foregroundIdentityStable && foregroundAXFrontmost,
    "frontWindowID": frontWindowIdentifier(for: foregroundPID),
    "screenCaptureReady": CGPreflightScreenCaptureAccess(),
    "targetFocusedWindowID": focusedWindowIdentifier(for: targetPID),
]
let data = try JSONSerialization.data(withJSONObject: probe, options: [.sortedKeys])
print(String(decoding: data, as: UTF8.self))
