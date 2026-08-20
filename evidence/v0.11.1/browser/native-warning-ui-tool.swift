import AppKit
import ApplicationServices
import Foundation

struct Candidate: Codable {
    let role: String
    let title: String
    let description: String
    let value: String
    let x: Double?
    let y: Double?
    let width: Double?
    let height: Double?
}

struct Interaction: Codable {
    let targetPid: Int32
    let query: String
    let exactButtonMatches: Int
    let button: Candidate
    let clickCenterX: Double
    let clickCenterY: Double
    let requestedForegroundPidAfterClick: Int32?
    let requestedCursorXAfterClick: Double?
    let requestedCursorYAfterClick: Double?
    let foregroundPidAfterClick: Int32?
    let cursorXAfterClick: Double?
    let cursorYAfterClick: Double?
    let foregroundRestorationDeferredUntilDisposableChromeExit: Bool
}

func stringAttribute(_ element: AXUIElement, _ attribute: CFString) -> String {
    var raw: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, attribute, &raw) == .success,
          let raw
    else { return "" }
    if let value = raw as? String { return value }
    if let value = raw as? NSAttributedString { return value.string }
    return ""
}

func pointAttribute(_ element: AXUIElement, _ attribute: CFString) -> CGPoint? {
    var raw: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, attribute, &raw) == .success,
          let raw,
          CFGetTypeID(raw) == AXValueGetTypeID()
    else { return nil }
    let value = unsafeBitCast(raw, to: AXValue.self)
    var point = CGPoint.zero
    guard AXValueGetType(value) == .cgPoint,
          AXValueGetValue(value, .cgPoint, &point)
    else { return nil }
    return point
}

func sizeAttribute(_ element: AXUIElement, _ attribute: CFString) -> CGSize? {
    var raw: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, attribute, &raw) == .success,
          let raw,
          CFGetTypeID(raw) == AXValueGetTypeID()
    else { return nil }
    let value = unsafeBitCast(raw, to: AXValue.self)
    var size = CGSize.zero
    guard AXValueGetType(value) == .cgSize,
          AXValueGetValue(value, .cgSize, &size)
    else { return nil }
    return size
}

func children(_ element: AXUIElement) -> [AXUIElement] {
    var raw: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, kAXChildrenAttribute as CFString, &raw) == .success,
          let children = raw as? [AXUIElement]
    else { return [] }
    return children
}

func collect(_ root: AXUIElement, matching query: String) -> [(AXUIElement, Candidate)] {
    var queue = [root]
    var output: [(AXUIElement, Candidate)] = []
    var visited = 0
    let normalizedQuery = query.lowercased()
    while !queue.isEmpty && visited < 20_000 {
        let element = queue.removeFirst()
        visited += 1
        let role = stringAttribute(element, kAXRoleAttribute as CFString)
        let title = stringAttribute(element, kAXTitleAttribute as CFString)
        let description = stringAttribute(element, kAXDescriptionAttribute as CFString)
        let value = stringAttribute(element, kAXValueAttribute as CFString)
        let haystack = "\(role)\n\(title)\n\(description)\n\(value)".lowercased()
        if normalizedQuery.isEmpty || haystack.contains(normalizedQuery) {
            let point = pointAttribute(element, kAXPositionAttribute as CFString)
            let size = sizeAttribute(element, kAXSizeAttribute as CFString)
            output.append((element, Candidate(
                role: role,
                title: title,
                description: description,
                value: value,
                x: point.map { Double($0.x) },
                y: point.map { Double($0.y) },
                width: size.map { Double($0.width) },
                height: size.map { Double($0.height) }
            )))
        }
        queue.append(contentsOf: children(element))
    }
    return output
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("\(message)\n".utf8))
    exit(2)
}

guard AXIsProcessTrusted() else { fail("Accessibility permission is not available; no prompt was requested") }
guard CommandLine.arguments.count >= 4,
      let pid = pid_t(CommandLine.arguments[2]),
      pid > 0
else { fail("usage: native-warning-ui-tool inspect|click PID QUERY [RESTORE_PID RESTORE_X RESTORE_Y OUTPUT_JSON]") }

let mode = CommandLine.arguments[1]
let query = CommandLine.arguments[3]
let app = AXUIElementCreateApplication(pid)
let matches = collect(app, matching: query)

if mode == "inspect" {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    print(String(data: try encoder.encode(matches.map(\.1)), encoding: .utf8)!)
    exit(0)
}

guard mode == "click" else { fail("unknown mode: \(mode)") }
let clickable = matches.filter { element, candidate in
    candidate.role == (kAXButtonRole as String)
        && [candidate.title, candidate.description, candidate.value].contains(query)
        && pointAttribute(element, kAXPositionAttribute as CFString) != nil
        && sizeAttribute(element, kAXSizeAttribute as CFString) != nil
}
guard clickable.count == 1 else {
    fail("refusing ambiguous click: found \(clickable.count) exact AXButton matches for \(query.debugDescription)")
}

let element = clickable[0].0
guard let origin = pointAttribute(element, kAXPositionAttribute as CFString),
      let size = sizeAttribute(element, kAXSizeAttribute as CFString),
      size.width >= 8,
      size.height >= 8
else { fail("exact button has no trustworthy on-screen bounds") }
let clickPoint = CGPoint(x: origin.x + size.width / 2, y: origin.y + size.height / 2)
let restorePid = CommandLine.arguments.count >= 5 ? pid_t(CommandLine.arguments[4]) : nil
let restoreX = CommandLine.arguments.count >= 6 ? Double(CommandLine.arguments[5]) : nil
let restoreY = CommandLine.arguments.count >= 7 ? Double(CommandLine.arguments[6]) : nil

guard let target = NSRunningApplication(processIdentifier: pid), !target.isTerminated else {
    fail("target process is not running")
}
target.activate(options: [.activateAllWindows])
usleep(250_000)
guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid else {
    fail("target process did not become the frontmost application")
}

func post(_ type: CGEventType, button: CGMouseButton) {
    guard let event = CGEvent(mouseEventSource: nil, mouseType: type, mouseCursorPosition: clickPoint, mouseButton: button) else {
        fail("could not create a CoreGraphics mouse event")
    }
    event.post(tap: .cghidEventTap)
}

post(.mouseMoved, button: .left)
usleep(100_000)
post(.leftMouseDown, button: .left)
usleep(80_000)
post(.leftMouseUp, button: .left)
usleep(250_000)

if let restoreX, let restoreY {
    CGWarpMouseCursorPosition(CGPoint(x: restoreX, y: restoreY))
}
if let restorePid,
   restorePid > 0,
   restorePid != pid,
   let previous = NSRunningApplication(processIdentifier: restorePid),
   !previous.isTerminated {
    previous.activate(options: [.activateAllWindows])
}
usleep(250_000)

let response = Candidate(
    role: clickable[0].1.role,
    title: clickable[0].1.title,
    description: clickable[0].1.description,
    value: clickable[0].1.value,
    x: Double(clickPoint.x),
    y: Double(clickPoint.y),
    width: Double(size.width),
    height: Double(size.height)
)
let encoder = JSONEncoder()
encoder.outputFormatting = [.sortedKeys]
print(String(data: try encoder.encode(response), encoding: .utf8)!)

if CommandLine.arguments.count >= 8 {
    let cursorAfter = CGEvent(source: nil)?.location
    let interaction = Interaction(
        targetPid: pid,
        query: query,
        exactButtonMatches: clickable.count,
        button: clickable[0].1,
        clickCenterX: Double(clickPoint.x),
        clickCenterY: Double(clickPoint.y),
        requestedForegroundPidAfterClick: restorePid,
        requestedCursorXAfterClick: restoreX,
        requestedCursorYAfterClick: restoreY,
        foregroundPidAfterClick: NSWorkspace.shared.frontmostApplication?.processIdentifier,
        cursorXAfterClick: cursorAfter.map { Double($0.x) },
        cursorYAfterClick: cursorAfter.map { Double($0.y) },
        foregroundRestorationDeferredUntilDisposableChromeExit: restorePid == pid
    )
    let path = CommandLine.arguments[7]
    try encoder.encode(interaction).write(to: URL(fileURLWithPath: path), options: .atomic)
}
