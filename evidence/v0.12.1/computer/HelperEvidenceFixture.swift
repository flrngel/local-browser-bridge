import AppKit
import Foundation

private let fixtureTitle = "LBB v0.12.1 Persistent SCStream Evidence"

private struct FixtureState: Codable {
    var pid: Int32
    var clicks = 0
    var semanticPresses = 0
    var semanticValue = ""
    var animationTick = 0
    var resizeCount = 0
    var focusCount = 0
    var appliedControlSequence = 0
    var contentWidth = 720
    var contentHeight = 460
    var lastAction = "ready"
}

private struct FixtureControl: Codable {
    let sequence: Int
    let action: String
    let contentWidth: Int?
    let contentHeight: Int?
}

private final class FixtureView: NSView, NSTextFieldDelegate {
    private let stateURL: URL
    private var state = FixtureState(pid: ProcessInfo.processInfo.processIdentifier)
    private let semanticField = NSTextField()

    init(frame: NSRect, stateURL: URL) {
        self.stateURL = stateURL
        super.init(frame: frame)

        autoresizingMask = [.width, .height]

        semanticField.frame = NSRect(x: 28, y: 154, width: 410, height: 34)
        semanticField.placeholderString = "Semantic value"
        semanticField.setAccessibilityLabel("Semantic value")
        semanticField.delegate = self
        addSubview(semanticField)

        let semanticButton = NSButton(
            title: "Semantic action",
            target: self,
            action: #selector(semanticAction(_:))
        )
        semanticButton.frame = NSRect(x: 452, y: 154, width: 220, height: 34)
        semanticButton.setAccessibilityLabel("Semantic action")
        addSubview(semanticButton)

        Timer.scheduledTimer(
            timeInterval: 0.1,
            target: self,
            selector: #selector(advanceFixture),
            userInfo: nil,
            repeats: true
        )
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
        synchronizeSemanticValue()
    }

    func recordResize(contentSize: NSSize, controlSequence: Int) {
        state.resizeCount += 1
        state.appliedControlSequence = controlSequence
        state.contentWidth = Int(contentSize.width.rounded())
        state.contentHeight = Int(contentSize.height.rounded())
        state.lastAction = "resize"
        persistAndRedraw()
    }

    func focusSemanticField(controlSequence: Int) -> Bool {
        guard let window, window.makeFirstResponder(semanticField) else { return false }
        if let editor = window.fieldEditor(false, for: semanticField) as? NSTextView {
            editor.setSelectedRange(NSRange(
                location: (semanticField.stringValue as NSString).length,
                length: 0
            ))
        }
        state.focusCount += 1
        state.appliedControlSequence = controlSequence
        state.lastAction = "focus-field"
        persistAndRedraw()
        return true
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor(calibratedRed: 0.07, green: 0.09, blue: 0.15, alpha: 1).setFill()
        dirtyRect.fill()

        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedSystemFont(ofSize: 20, weight: .medium),
            .foregroundColor: NSColor.white,
        ]
        "Local Browser Bridge — v0.12.1 persistent SCStream".draw(
            at: NSPoint(x: 28, y: bounds.height - 58),
            withAttributes: attributes
        )
        "clicks=\(state.clicks)  semantic=\(state.semanticPresses)  tick=\(state.animationTick)".draw(
            at: NSPoint(x: 28, y: bounds.height - 108),
            withAttributes: attributes
        )
        "value=\(state.semanticValue)".draw(
            at: NSPoint(x: 28, y: bounds.height - 156),
            withAttributes: attributes
        )
        "last=\(state.lastAction)  size=\(state.contentWidth)x\(state.contentHeight)".draw(
            at: NSPoint(x: 28, y: bounds.height - 204),
            withAttributes: attributes
        )

        NSColor.systemBlue.setFill()
        NSBezierPath(
            roundedRect: NSRect(x: 28, y: 42, width: bounds.width - 56, height: 90),
            xRadius: 16,
            yRadius: 16
        ).fill()
        "Background pixel click target".draw(
            at: NSPoint(x: 52, y: 76),
            withAttributes: attributes
        )

        let travel = max(1, Int(bounds.width) - 104)
        let markerX = 52 + (state.animationTick * 17) % travel
        NSColor.systemGreen.setFill()
        NSBezierPath(ovalIn: NSRect(x: markerX, y: 112, width: 14, height: 14)).fill()
    }

    private func persistAndRedraw() {
        writeState()
        needsDisplay = true
        displayIfNeeded()
    }

    private func synchronizeSemanticValue() {
        guard state.semanticValue != semanticField.stringValue else { return }
        state.semanticValue = semanticField.stringValue
        state.lastAction = "set-value"
        persistAndRedraw()
    }

    @objc private func advanceFixture() {
        synchronizeSemanticValue()
        state.animationTick += 1
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
    private var fixtureView: FixtureView?
    private var controlURL: URL?
    private var lastControlSequence = 0

    func applicationDidFinishLaunching(_ notification: Notification) {
        let environment = ProcessInfo.processInfo.environment
        guard
            let statePath = environment["LBB_FIXTURE_STATE"],
            let controlPath = environment["LBB_FIXTURE_CONTROL"]
        else {
            fputs("LBB_FIXTURE_STATE and LBB_FIXTURE_CONTROL are required\n", stderr)
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
        window.title = fixtureTitle
        window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: 700, height: 440)
        window.maxSize = NSSize(width: 900, height: 620)

        let view = FixtureView(
            frame: NSRect(origin: .zero, size: contentRect.size),
            stateURL: URL(fileURLWithPath: statePath)
        )
        window.contentView = view
        window.makeFirstResponder(view)
        window.orderFrontRegardless()

        self.window = window
        fixtureView = view
        controlURL = URL(fileURLWithPath: controlPath)

        Timer.scheduledTimer(
            timeInterval: 0.1,
            target: self,
            selector: #selector(pollControl),
            userInfo: nil,
            repeats: true
        )
    }

    @objc private func pollControl() {
        guard let controlURL, let window, let fixtureView else { return }
        guard
            let data = try? Data(contentsOf: controlURL),
            let control = try? JSONDecoder().decode(FixtureControl.self, from: data),
            control.sequence > lastControlSequence
        else { return }

        if control.action == "focus-semantic-field" {
            guard fixtureView.focusSemanticField(controlSequence: control.sequence) else {
                fputs("fixture could not focus its semantic field\n", stderr)
                return
            }
            lastControlSequence = control.sequence
            return
        }

        guard control.action == "resize",
              let width = control.contentWidth,
              let height = control.contentHeight,
              (700 ... 900).contains(width),
              (440 ... 620).contains(height)
        else {
            fputs("fixture rejected invalid control command\n", stderr)
            return
        }

        lastControlSequence = control.sequence
        let size = NSSize(width: width, height: height)
        window.setContentSize(size)
        window.displayIfNeeded()
        fixtureView.recordResize(contentSize: window.contentLayoutRect.size, controlSequence: control.sequence)
    }
}

private let app = NSApplication.shared
private let delegate = FixtureDelegate()
app.setActivationPolicy(.accessory)
app.delegate = delegate
app.run()
