import AppKit
import Foundation

private struct FixtureState: Codable {
    var pid: Int32
    var clicks = 0
    var drags = 0
    var scroll = 0
    var semanticPresses = 0
    var text = ""
    var lastAction = "ready"
}

private final class FixtureView: NSView {
    private let stateURL: URL
    private var state = FixtureState(pid: ProcessInfo.processInfo.processIdentifier)

    init(frame: NSRect, stateURL: URL) {
        self.stateURL = stateURL
        super.init(frame: frame)
        let semanticField = NSTextField(frame: NSRect(x: 28, y: 150, width: 410, height: 34))
        semanticField.placeholderString = "Semantic text"
        semanticField.setAccessibilityLabel("Semantic text")
        addSubview(semanticField)

        let semanticButton = NSButton(
            title: "Semantic action",
            target: self,
            action: #selector(semanticAction(_:))
        )
        semanticButton.frame = NSRect(x: 452, y: 150, width: 220, height: 34)
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

    override func mouseDragged(with event: NSEvent) {
        state.drags += 1
        state.lastAction = "drag"
        persistAndRedraw()
    }

    override func scrollWheel(with event: NSEvent) {
        let delta = Int(event.scrollingDeltaY.rounded())
        state.scroll += delta == 0 ? (event.deltaY >= 0 ? 1 : -1) : delta
        state.lastAction = "scroll"
        persistAndRedraw()
    }

    override func keyDown(with event: NSEvent) {
        if event.keyCode == 36 {
            state.text += "[enter]"
        } else {
            state.text += event.characters ?? ""
        }
        state.lastAction = "key"
        persistAndRedraw()
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor(calibratedRed: 0.08, green: 0.10, blue: 0.16, alpha: 1).setFill()
        dirtyRect.fill()

        let title = "Local Browser Bridge — Background Input Fixture"
        let status = "clicks=\(state.clicks)  drags=\(state.drags)  scroll=\(state.scroll)  semantic=\(state.semanticPresses)"
        let text = "text=\(state.text)"
        let action = "last=\(state.lastAction)"
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedSystemFont(ofSize: 20, weight: .medium),
            .foregroundColor: NSColor.white,
        ]
        title.draw(at: NSPoint(x: 28, y: bounds.height - 58), withAttributes: attributes)
        status.draw(at: NSPoint(x: 28, y: bounds.height - 118), withAttributes: attributes)
        text.draw(at: NSPoint(x: 28, y: bounds.height - 168), withAttributes: attributes)
        action.draw(at: NSPoint(x: 28, y: bounds.height - 218), withAttributes: attributes)

        NSColor.systemBlue.setFill()
        NSBezierPath(roundedRect: NSRect(x: 28, y: 42, width: bounds.width - 56, height: 90), xRadius: 16, yRadius: 16).fill()
        "Background click / drag / scroll target".draw(
            at: NSPoint(x: 52, y: 76),
            withAttributes: attributes
        )
    }

    private func persistAndRedraw() {
        writeState()
        needsDisplay = true
        displayIfNeeded()
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
        window.title = "LBB Background Fixture"
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
