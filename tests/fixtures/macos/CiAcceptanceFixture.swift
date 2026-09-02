import AppKit
import Foundation

// Deterministic native target window for CI-hosted computer acceptance.
//
// The fixture is a regular, self-activating AppKit application so that a fresh
// hosted-runner session has a frontmost process with a real focused window
// before the helper's exact-window invariants run. It exposes one text field
// with an accessibility ValuePattern equivalent, one push button, one pixel
// click surface, and one focused text recipient. Every state change is written
// atomically as JSON to the path in `LBB_FIXTURE_STATE`. No environment,
// command line, path, or credential is ever written.

private let fixtureTitle = "LBB CI Acceptance Fixture"

private struct FixtureState: Codable {
    var schemaVersion = 1
    var pid = ProcessInfo.processInfo.processIdentifier
    var windowNumber = 0
    var clicks = 0
    var invokeCount = 0
    var semanticValue = ""
    var focusedText = ""
    var keyWindow = false
    var lastAction = "ready"
}

private final class ClickSurface: NSView {
    var onClick: (() -> Void)?

    override var acceptsFirstResponder: Bool { true }

    override func isAccessibilityElement() -> Bool { true }

    override func accessibilityRole() -> NSAccessibility.Role? { .button }

    override func accessibilityLabel() -> String? { "Pixel Input Surface" }

    override func accessibilityIdentifier() -> String { "PixelInputSurface" }

    override func accessibilityPerformPress() -> Bool {
        onClick?()
        needsDisplay = true
        return true
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        onClick?()
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.systemBlue.setFill()
        NSBezierPath(roundedRect: bounds, xRadius: 12, yRadius: 12).fill()
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedSystemFont(ofSize: 15, weight: .bold),
            .foregroundColor: NSColor.white,
        ]
        "Pixel click target".draw(at: NSPoint(x: 18, y: bounds.height / 2 - 9), withAttributes: attributes)
    }
}

private final class FixtureController: NSObject, NSApplicationDelegate, NSTextFieldDelegate {
    private let stateURL: URL
    private var state = FixtureState()
    private var window: NSWindow?
    private let valueField = NSTextField()
    private let focusedField = NSTextField()
    private let counterLabel = NSTextField(labelWithString: "Count: 0")

    init(stateURL: URL) {
        self.stateURL = stateURL
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        let contentRect = NSRect(x: 160, y: 160, width: 720, height: 420)
        let window = NSWindow(
            contentRect: contentRect,
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = fixtureTitle
        window.isReleasedWhenClosed = false
        let content = NSView(frame: NSRect(origin: .zero, size: contentRect.size))
        content.wantsLayer = true
        content.layer?.backgroundColor = NSColor(calibratedWhite: 0.96, alpha: 1).cgColor
        window.contentView = content

        let heading = NSTextField(labelWithString: fixtureTitle)
        heading.font = NSFont.boldSystemFont(ofSize: 20)
        heading.frame = NSRect(x: 24, y: 372, width: 600, height: 28)
        heading.setAccessibilityLabel("Fixture Heading")
        content.addSubview(heading)

        valueField.frame = NSRect(x: 24, y: 316, width: 360, height: 30)
        valueField.placeholderString = "Semantic value"
        valueField.stringValue = "initial-value"
        valueField.setAccessibilityLabel("Fixture Value Input")
        valueField.delegate = self
        content.addSubview(valueField)

        let button = NSButton(title: "Increment Counter", target: self, action: #selector(increment(_:)))
        button.frame = NSRect(x: 400, y: 314, width: 180, height: 34)
        button.setAccessibilityLabel("Increment Counter")
        content.addSubview(button)

        counterLabel.frame = NSRect(x: 592, y: 320, width: 110, height: 24)
        counterLabel.setAccessibilityLabel("Invocation Counter")
        content.addSubview(counterLabel)

        focusedField.frame = NSRect(x: 24, y: 262, width: 556, height: 30)
        focusedField.placeholderString = "Focused text input"
        focusedField.setAccessibilityLabel("Focused Text Input")
        focusedField.delegate = self
        content.addSubview(focusedField)

        let surface = ClickSurface(frame: NSRect(x: 24, y: 40, width: 672, height: 190))
        surface.onClick = { [weak self] in
            self?.state.clicks += 1
            self?.state.lastAction = "click"
            self?.writeState()
        }
        content.addSubview(surface)

        self.window = window
        state.windowNumber = window.windowNumber
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(focusedField)
        writeState()

        Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { [weak self] _ in
            self?.synchronize()
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { true }

    func controlTextDidChange(_ notification: Notification) {
        synchronize()
    }

    @objc private func increment(_ sender: NSButton) {
        state.invokeCount += 1
        state.lastAction = "invoke"
        counterLabel.stringValue = "Count: \(state.invokeCount)"
        writeState()
    }

    private func synchronize() {
        var changed = false
        if state.semanticValue != valueField.stringValue {
            state.semanticValue = valueField.stringValue
            state.lastAction = "set-value"
            changed = true
        }
        if state.focusedText != focusedField.stringValue {
            state.focusedText = focusedField.stringValue
            state.lastAction = "type"
            changed = true
        }
        let keyWindow = window?.isKeyWindow ?? false
        if state.keyWindow != keyWindow {
            state.keyWindow = keyWindow
            changed = true
        }
        if changed {
            writeState()
        }
    }

    private func writeState() {
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            try encoder.encode(state).write(to: stateURL, options: .atomic)
        } catch {
            fputs("fixture state write failed: \(error)\n", stderr)
        }
    }
}

guard let statePath = ProcessInfo.processInfo.environment["LBB_FIXTURE_STATE"], !statePath.isEmpty else {
    fputs("LBB_FIXTURE_STATE must name the fixture state file\n", stderr)
    exit(2)
}
private let app = NSApplication.shared
private let controller = FixtureController(stateURL: URL(fileURLWithPath: statePath))
app.setActivationPolicy(.regular)
app.delegate = controller
app.run()
