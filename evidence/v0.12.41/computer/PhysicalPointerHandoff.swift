import AppKit
import Darwin
import Foundation

private enum PromptState: String, CaseIterable {
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

    var heading: String {
        switch self {
        case .waiting: "WAITING FOR RUNNER"
        case .move: "MOVE THE POINTER NOW"
        case .action: "KEEP MOVING — ACTION RUNNING"
        case .complete: "COMPLETE — STOP MOVING"
        }
    }

    var instruction: String {
        switch self {
        case .waiting: "Do not click or move yet."
        case .move: "Move continuously without clicking. Keep moving until this panel turns green."
        case .action: "Keep moving without clicking while the bounded action finishes."
        case .complete: "The bounded pointer-concurrency cell finished."
        }
    }

    var background: NSColor {
        switch self {
        case .waiting: NSColor(calibratedRed: 0.31, green: 0.35, blue: 0.40, alpha: 1)
        case .move, .action: NSColor(calibratedRed: 1.00, green: 0.55, blue: 0.12, alpha: 1)
        case .complete: NSColor(calibratedRed: 0.23, green: 0.69, blue: 0.37, alpha: 1)
        }
    }

    func mayTransition(to next: PromptState) -> Bool {
        switch (self, next) {
        // ACTION may return to MOVE only while the runner still has definitive
        // pre-dispatch authority. The runner never writes that transition once
        // a product request exists.
        case (.waiting, .move), (.move, .action), (.action, .move), (.action, .complete): true
        default: false
        }
    }
}

private final class PassivePromptPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

private final class PromptController: NSObject, NSApplicationDelegate {
    private let controlPath: String
    private let armExpiresAt: Date
    private let hardExpiresAt: Date
    private let panel: PassivePromptPanel
    private let heading = NSTextField(labelWithString: "")
    private let instruction = NSTextField(wrappingLabelWithString: "")
    private let countdown = NSTextField(labelWithString: "")
    private var state = PromptState.waiting
    private var actionExpiresAt: Date?
    private var timer: Timer?

    init(controlPath: String, armExpiresAt: Date, hardExpiresAt: Date) {
        self.controlPath = controlPath
        self.armExpiresAt = armExpiresAt
        self.hardExpiresAt = hardExpiresAt
        panel = PassivePromptPanel(
            contentRect: NSRect(x: 0, y: 0, width: 520, height: 190),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        panel.hidesOnDeactivate = false
        panel.ignoresMouseEvents = true
        panel.acceptsMouseMovedEvents = false
        panel.hasShadow = true
        panel.isOpaque = true
        panel.isReleasedWhenClosed = false
        panel.setAccessibilityLabel("Local Browser Bridge pointer handoff notification")

        let content = NSView(frame: panel.contentView!.bounds)
        content.autoresizingMask = [.width, .height]
        content.wantsLayer = true
        panel.contentView = content

        heading.frame = NSRect(x: 24, y: 117, width: 472, height: 44)
        heading.font = NSFont.systemFont(ofSize: 25, weight: .bold)
        heading.alignment = .center
        heading.textColor = .white
        heading.isSelectable = false
        heading.setAccessibilityLabel("Pointer handoff state")
        content.addSubview(heading)

        instruction.frame = NSRect(x: 32, y: 54, width: 456, height: 58)
        instruction.font = NSFont.systemFont(ofSize: 16, weight: .semibold)
        instruction.alignment = .center
        instruction.textColor = .white
        instruction.maximumNumberOfLines = 2
        instruction.isSelectable = false
        instruction.setAccessibilityLabel("Pointer handoff instruction")
        content.addSubview(instruction)

        countdown.frame = NSRect(x: 24, y: 18, width: 472, height: 26)
        countdown.font = NSFont.monospacedDigitSystemFont(ofSize: 14, weight: .medium)
        countdown.alignment = .center
        countdown.textColor = .white
        countdown.isSelectable = false
        countdown.setAccessibilityLabel("Pointer handoff deadline")
        content.addSubview(countdown)

        updatePresentation(.waiting)
        positionPanel()
        panel.orderFrontRegardless()
        timer = Timer.scheduledTimer(withTimeInterval: 0.1, repeats: true) { [weak self] _ in
            self?.pollRunnerState()
        }
    }

    private func positionPanel() {
        let visible = NSScreen.main?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
        let origin = NSPoint(
            x: max(visible.minX + 20, visible.maxX - panel.frame.width - 24),
            y: max(visible.minY + 20, visible.maxY - panel.frame.height - 24)
        )
        panel.setFrameOrigin(origin)
    }

    private func pollRunnerState() {
        let priorExpiration = state == .waiting || state == .move
            ? armExpiresAt
            : actionExpiresAt ?? hardExpiresAt
        if priorExpiration.timeIntervalSinceNow <= 0 {
            timer?.invalidate()
            NSApp.terminate(nil)
            return
        }

        if FileManager.default.fileExists(atPath: controlPath) {
            do {
                let raw = try String(contentsOfFile: controlPath, encoding: .utf8)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                guard let next = PromptState(rawValue: raw) else {
                    throw NSError(domain: "PointerHandoff", code: 1)
                }
                if next != state {
                    guard state.mayTransition(to: next) else {
                        throw NSError(domain: "PointerHandoff", code: 2)
                    }
                    let transitionExpiration = state == .waiting || state == .move
                        ? armExpiresAt
                        : actionExpiresAt ?? hardExpiresAt
                    guard transitionExpiration.timeIntervalSinceNow > 0 else {
                        timer?.invalidate()
                        NSApp.terminate(nil)
                        return
                    }
                    if next == .action {
                        actionExpiresAt = min(
                            hardExpiresAt,
                            Date().addingTimeInterval(10)
                        )
                    } else if next == .move {
                        actionExpiresAt = nil
                    }
                    updatePresentation(next)
                }
            } catch {
                timer?.invalidate()
                fputs("Pointer handoff control state is unreadable or invalid.\n", stderr)
                NSApp.terminate(nil)
                return
            }
        }

        let expiration = state == .waiting || state == .move
            ? armExpiresAt
            : actionExpiresAt ?? hardExpiresAt
        let remaining = max(0, Int(expiration.timeIntervalSinceNow.rounded(.up)))
        countdown.stringValue = state == .complete
            ? "Closing in: \(remaining)s"
            : "Time remaining: \(remaining)s"
        if remaining == 0 {
            timer?.invalidate()
            NSApp.terminate(nil)
        }
    }

    private func updatePresentation(_ next: PromptState) {
        state = next
        panel.title = next.title
        panel.backgroundColor = next.background
        panel.contentView?.layer?.backgroundColor = next.background.cgColor
        heading.stringValue = next.heading
        instruction.stringValue = next.instruction
        panel.setAccessibilityTitle(next.title)
    }
}

private func publishCreateOnce(sourcePath: String, destinationPath: String) -> Bool {
    let renamed = sourcePath.withCString { source in
        destinationPath.withCString { destination in
            renameatx_np(
                AT_FDCWD,
                source,
                AT_FDCWD,
                destination,
                UInt32(RENAME_EXCL)
            ) == 0
        }
    }
    guard renamed else { return false }

    let directoryPath = (destinationPath as NSString).deletingLastPathComponent
    let directoryDescriptor = open(directoryPath, O_RDONLY)
    guard directoryDescriptor >= 0 else { return false }
    defer { close(directoryDescriptor) }
    return fsync(directoryDescriptor) == 0
}

private func runSelfTest() {
    precondition(PromptState.allCases.map(\.rawValue) == ["WAITING", "MOVE", "ACTION", "COMPLETE"])
    precondition(PromptState.waiting.title == "LBB macOS Acceptance - WAITING")
    precondition(PromptState.move.title == "LBB macOS Acceptance - MOVE POINTER")
    precondition(PromptState.action.title == "LBB macOS Acceptance - ACTION RUNNING")
    precondition(PromptState.complete.title == "LBB macOS Acceptance - COMPLETE")
    precondition(PromptState.waiting.mayTransition(to: .move))
    precondition(PromptState.move.mayTransition(to: .action))
    precondition(PromptState.action.mayTransition(to: .move))
    precondition(PromptState.action.mayTransition(to: .complete))
    precondition(!PromptState.waiting.mayTransition(to: .complete))
    let testDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("lbb-pointer-handoff-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: testDirectory) }
    do {
        try FileManager.default.createDirectory(
            at: testDirectory,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        let firstSource = testDirectory.appendingPathComponent("first-source")
        let secondSource = testDirectory.appendingPathComponent("second-source")
        let destination = testDirectory.appendingPathComponent("published")
        try Data("first\n".utf8).write(to: firstSource)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: firstSource.path
        )
        precondition(publishCreateOnce(
            sourcePath: firstSource.path,
            destinationPath: destination.path
        ))
        try Data("second\n".utf8).write(to: secondSource)
        precondition(!publishCreateOnce(
            sourcePath: secondSource.path,
            destinationPath: destination.path
        ))
        let published = try Data(contentsOf: destination)
        let publishedAttributes = try FileManager.default.attributesOfItem(
            atPath: destination.path
        )
        precondition(published == Data("first\n".utf8))
        precondition((publishedAttributes[.referenceCount] as? NSNumber)?.intValue == 1)
        precondition((publishedAttributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)
    } catch {
        preconditionFailure("pointer handoff filesystem self-test failed")
    }
    print("macOS pointer handoff prompt self-test passed")
}

let arguments = Array(CommandLine.arguments.dropFirst())
if arguments == ["--self-test"] {
    runSelfTest()
    exit(0)
}
if arguments.count == 3, arguments[0] == "--publish-create-once" {
    guard arguments[1].hasPrefix("/"), arguments[2].hasPrefix("/"),
          arguments[1] != arguments[2],
          publishCreateOnce(sourcePath: arguments[1], destinationPath: arguments[2])
    else {
        fputs("Create-once pointer handoff publication failed.\n", stderr)
        exit(3)
    }
    exit(0)
}
guard arguments.count == 3,
      arguments[0].hasPrefix("/"),
      let armExpirationMilliseconds = Double(arguments[1]),
      armExpirationMilliseconds.isFinite,
      let hardExpirationMilliseconds = Double(arguments[2]),
      hardExpirationMilliseconds.isFinite
else {
    fputs("Usage: pointer-handoff <absolute-control-path> <arm-expiration-epoch-ms> <hard-expiration-epoch-ms>\n", stderr)
    exit(2)
}

let armExpiration = Date(timeIntervalSince1970: armExpirationMilliseconds / 1_000)
let hardExpiration = Date(timeIntervalSince1970: hardExpirationMilliseconds / 1_000)
let armRemaining = armExpiration.timeIntervalSinceNow
let hardRemaining = hardExpiration.timeIntervalSinceNow
let completionGrace = hardExpiration.timeIntervalSince(armExpiration)
guard armRemaining > 0, armRemaining <= 300,
      hardRemaining > armRemaining, hardRemaining <= 310,
      completionGrace > 0, completionGrace <= 10
else {
    fputs("Pointer handoff deadlines are outside the accepted windows.\n", stderr)
    exit(2)
}

let application = NSApplication.shared
application.setActivationPolicy(.accessory)
private let controller = PromptController(
    controlPath: arguments[0],
    armExpiresAt: armExpiration,
    hardExpiresAt: hardExpiration
)
application.delegate = controller
application.run()
