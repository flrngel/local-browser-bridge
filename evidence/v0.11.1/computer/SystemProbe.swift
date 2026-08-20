import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

let cursor = CGEvent(source: nil)?.location ?? .zero
let probe: [String: Any] = [
    "accessibilityReady": AXIsProcessTrusted(),
    "cursorX": cursor.x,
    "cursorY": cursor.y,
    "foregroundApplication": NSWorkspace.shared.frontmostApplication?.localizedName ?? "unknown",
    "screenCaptureReady": CGPreflightScreenCaptureAccess(),
]
let data = try JSONSerialization.data(withJSONObject: probe, options: [.sortedKeys])
print(String(decoding: data, as: UTF8.self))
