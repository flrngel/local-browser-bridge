import AppKit
import Foundation

private struct FixtureState: Codable {
    var pid: Int32
    var clicks = 0
    var semanticPresses = 0
    var semanticValue = ""
    var lastAction = "ready"
}

private final class FixtureView: NSView, NSTextFieldDelegate {
    private let stateURL: URL
    private var state = FixtureState(pid: ProcessInfo.processInfo.processIdentifier)
    private let semanticField = NSTextField()

    init(frame: NSRect, stateURL: URL) {
        self.stateURL = stateURL
        super.init(frame: frame)

        semanticField.frame = NSRect(x: 28, y: 154, width: 410, height: 34)
        semanticField.placeholderString = "Semantic value"
        semanticField.setAccessibilityLabel("Semantic value")
        semanticField.delegate = self
        addSubview(semanticField)
        Timer.scheduledTimer(
            timeInterval: 0.1,
            target: self,
            selector: #selector(syncSemanticValue),
            userInfo: nil,
            repeats: true
        )

        let semanticButton = NSButton(
            title: "Semantic action",
            target: self,
            action: #selector(semanticAction(_:))
        )
        semanticButton.frame = NSRect(x: 452, y: 154, width: 220, height: 34)
        semanticButton.setAccessibilityLabel("Semantic action")
        addSubview(semanticButton)
        writeState()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override var acceptsFirstResponder: Bool { true }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        state.clicks += 1
        state.lastAction = "click"
        persistAndRedraw()
    }

    func controlTextDidChange(_ notification: Notification) {
        state.semanticValue = semanticField.stringValue
        state.lastAction = "set-value"
        persistAndRedraw()
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor(calibratedRed: 0.07, green: 0.09, blue: 0.15, alpha: 1).setFill()
        dirtyRect.fill()

        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedSystemFont(ofSize: 20, weight: .medium),
            .foregroundColor: NSColor.white,
        ]
        "Local Browser Bridge — v0.11.1 packaged helper".draw(
            at: NSPoint(x: 28, y: bounds.height - 58),
            withAttributes: attributes
        )
        "clicks=\(state.clicks)  semantic=\(state.semanticPresses)".draw(
            at: NSPoint(x: 28, y: bounds.height - 112),
            withAttributes: attributes
        )
        "value=\(state.semanticValue)".draw(
            at: NSPoint(x: 28, y: bounds.height - 160),
            withAttributes: attributes
        )
        "last=\(state.lastAction)".draw(
            at: NSPoint(x: 28, y: bounds.height - 208),
            withAttributes: attributes
        )

        NSColor.systemBlue.setFill()
        NSBezierPath(
            roundedRect: NSRect(x: 28, y: 42, width: bounds.width - 56, height: 90),
            xRadius: 16,
            yRadius: 16
        ).fill()
        "Pixel click target".draw(
            at: NSPoint(x: 52, y: 76),
            withAttributes: attributes
        )
    }

    private func persistAndRedraw() {
        writeState()
        needsDisplay = true
        displayIfNeeded()
    }

    @objc private func syncSemanticValue() {
        guard state.semanticValue != semanticField.stringValue else { return }
        state.semanticValue = semanticField.stringValue
        state.lastAction = "set-value"
        persistAndRedraw()
    }

    @objc private func semanticAction(_ sender: NSButton) {
        state.semanticPresses += 1
        state.lastAction = "semantic"
        sender.title = "Semantic action complete"
        sender.setAccessibilityLabel("Semantic action complete")
        persistAndRedraw()
    }

    private func writeState() {
        do {
            let data = try JSONEncoder().encode(state)
            try data.write(to: stateURL, options: .atomic)
        } catch {
            fputs("fixture state write failed: \(error)\n", stderr)
        }
    }
}

private final class FixtureDelegate: NSObject, NSApplicationDelegate {
    private var window: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        guard let statePath = ProcessInfo.processInfo.environment["LBB_FIXTURE_STATE"] else {
            fputs("LBB_FIXTURE_STATE is required\n", stderr)
            NSApp.terminate(nil)
            return
        }
        let contentRect = NSRect(x: 180, y: 180, width: 720, height: 460)
        let window = NSWindow(
            contentRect: contentRect,
            styleMask: [.titled, .closable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.title = "LBB v0.11.1 Helper Evidence"
        window.isReleasedWhenClosed = false
        let view = FixtureView(
            frame: NSRect(origin: .zero, size: contentRect.size),
            stateURL: URL(fileURLWithPath: statePath)
        )
        window.contentView = view
        window.makeFirstResponder(view)
        window.orderFrontRegardless()
        self.window = window
    }
}

private let app = NSApplication.shared
private let delegate = FixtureDelegate()
app.setActivationPolicy(.accessory)
app.delegate = delegate
app.run()
