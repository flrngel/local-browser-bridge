import AppKit
import CryptoKit
import Darwin
import Foundation

private let productVersion = "0.12.53"
private let stableWindowTitle = "LBB macOS Acceptance App Share"
private let readyButtonTitle = "START APP-SHARE CHECK"
private let armedButtonTitle = "APP-SHARE CHECK ARMED"
private let actionButtonTitle = "APP-SHARE CHECK RUNNING"
private let completeButtonTitle = "APP-SHARE CHECK COMPLETE"
private let armWindowSeconds: TimeInterval = 300
private let completionGraceSeconds: TimeInterval = 18

private enum HandoffState: String, CaseIterable {
    case waiting = "WAITING"
    case ready = "READY"
    case armed = "ARMED"
    case action = "ACTION"
    case complete = "COMPLETE"

    var heading: String {
        switch self {
        case .waiting: "WAITING FOR RUNNER"
        case .ready: "APP-SHARE ACTION REQUIRED"
        case .armed: "APP-SHARE ACTION RECEIVED"
        case .action: "PRODUCT ACTION RUNNING"
        case .complete: "APP-SHARE CHECK COMPLETE"
        }
    }

    var instruction: String {
        switch self {
        case .waiting: "The release-candidate runner is preparing a bound request."
        case .ready: "Use the separately authorized exact-app share to press the button once."
        case .armed: "Do not press again. The runner is verifying the isolated action."
        case .action: "The bounded product action is running without using the shared input seat."
        case .complete: "The exact-app-share concurrency cell completed."
        }
    }

    var background: NSColor {
        switch self {
        case .waiting: NSColor(calibratedRed: 0.31, green: 0.35, blue: 0.40, alpha: 1)
        case .ready: NSColor(calibratedRed: 1.00, green: 0.55, blue: 0.12, alpha: 1)
        case .armed, .action: NSColor(calibratedRed: 0.17, green: 0.45, blue: 0.82, alpha: 1)
        case .complete: NSColor(calibratedRed: 0.23, green: 0.69, blue: 0.37, alpha: 1)
        }
    }
}

private struct ControlRecord {
    let state: HandoffState
    let requestSha256: String
    let startReceiptSha256: String?
    let productActionStartedAt: String?
    let productActionCompletedAt: String?
}

private struct HandoffInvocation {
    let armExpiration: Date
    let hardExpiration: Date
}

private final class NonactivatingHandoffPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

private func isCanonicalSha256(_ value: String) -> Bool {
    value.range(of: "^[0-9a-f]{64}$", options: .regularExpression) != nil
}

private func isCanonicalRequestId(_ value: String) -> Bool {
    value.range(of: "^[0-9a-f]{32}$", options: .regularExpression) != nil
}

private func canonicalTimestamp() -> String {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter.string(from: Date())
}

private func parseCanonicalTimestamp(_ value: String) -> Date? {
    guard value.range(
        of: "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\\.[0-9]{3}Z$",
        options: .regularExpression
    ) != nil else { return nil }
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    guard let date = formatter.date(from: value), formatter.string(from: date) == value else {
        return nil
    }
    return date
}

private func validateHandoffInvocation(
    armExpirationMilliseconds: Double,
    hardExpirationMilliseconds: Double,
    now: Date = Date()
) -> HandoffInvocation? {
    guard armExpirationMilliseconds.isFinite,
          hardExpirationMilliseconds.isFinite
    else { return nil }
    let armExpiration = Date(timeIntervalSince1970: armExpirationMilliseconds / 1_000)
    let hardExpiration = Date(timeIntervalSince1970: hardExpirationMilliseconds / 1_000)
    let armRemaining = armExpiration.timeIntervalSince(now)
    let hardRemaining = hardExpiration.timeIntervalSince(now)
    let completionGrace = hardExpiration.timeIntervalSince(armExpiration)
    guard armRemaining > 0,
          armRemaining <= armWindowSeconds,
          hardRemaining > armRemaining,
          hardRemaining <= armWindowSeconds + completionGraceSeconds,
          completionGrace > 0,
          completionGrace <= completionGraceSeconds
    else { return nil }
    return HandoffInvocation(
        armExpiration: armExpiration,
        hardExpiration: hardExpiration
    )
}

private func secureSha256(path: String) -> String? {
    let descriptor = open(path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK)
    guard descriptor >= 0 else { return nil }
    var metadata = stat()
    var pathMetadata = stat()
    guard fstat(descriptor, &metadata) == 0,
          lstat(path, &pathMetadata) == 0,
          ownerPrivateOrdinaryFile(metadata, maximumBytes: 16 * 1024),
          sameStableFileIdentity(metadata, pathMetadata)
    else {
        close(descriptor)
        return nil
    }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: true)
    let data = handle.readDataToEndOfFile()
    var metadataAfter = stat()
    var pathMetadataAfter = stat()
    guard fstat(descriptor, &metadataAfter) == 0,
          lstat(path, &pathMetadataAfter) == 0,
          Int64(data.count) == Int64(metadata.st_size),
          sameStableFileIdentity(metadata, metadataAfter),
          sameStableFileIdentity(metadataAfter, pathMetadataAfter)
    else { return nil }
    return SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

private func canonicalJSON(_ value: [String: Any]) throws -> Data {
    guard JSONSerialization.isValidJSONObject(value) else {
        throw NSError(domain: "AppShareHandoff", code: 1)
    }
    var data = try JSONSerialization.data(withJSONObject: value, options: [.sortedKeys])
    data.append(0x0a)
    return data
}

private func sha256(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
}

private func sameStableFileIdentity(_ left: stat, _ right: stat) -> Bool {
    left.st_dev == right.st_dev &&
        left.st_ino == right.st_ino &&
        left.st_mode == right.st_mode &&
        left.st_nlink == right.st_nlink &&
        left.st_uid == right.st_uid &&
        left.st_size == right.st_size &&
        left.st_mtimespec.tv_sec == right.st_mtimespec.tv_sec &&
        left.st_mtimespec.tv_nsec == right.st_mtimespec.tv_nsec &&
        left.st_ctimespec.tv_sec == right.st_ctimespec.tv_sec &&
        left.st_ctimespec.tv_nsec == right.st_ctimespec.tv_nsec
}

private func ownerPrivateOrdinaryFile(_ metadata: stat, maximumBytes: Int64) -> Bool {
    (metadata.st_mode & S_IFMT) == S_IFREG &&
        metadata.st_nlink == 1 &&
        metadata.st_uid == getuid() &&
        (metadata.st_mode & 0o077) == 0 &&
        metadata.st_size > 0 &&
        metadata.st_size <= maximumBytes
}

private enum SecureTextRead {
    case missing
    case changed
    case invalid
    case value(String)
}

private func secureReadText(path: String, maximumBytes: Int64) -> SecureTextRead {
    let descriptor = open(path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK)
    guard descriptor >= 0 else {
        return errno == ENOENT ? .missing : .invalid
    }
    defer { close(descriptor) }
    var before = stat()
    var pathBefore = stat()
    guard fstat(descriptor, &before) == 0,
          lstat(path, &pathBefore) == 0,
          ownerPrivateOrdinaryFile(before, maximumBytes: maximumBytes),
          sameStableFileIdentity(before, pathBefore)
    else { return .invalid }
    let handle = FileHandle(fileDescriptor: descriptor, closeOnDealloc: false)
    let data = handle.readDataToEndOfFile()
    var after = stat()
    var pathAfter = stat()
    guard fstat(descriptor, &after) == 0,
          lstat(path, &pathAfter) == 0
    else { return .changed }
    guard sameStableFileIdentity(before, after),
          sameStableFileIdentity(after, pathAfter),
          Int64(data.count) == before.st_size
    else { return .changed }
    guard !data.contains(0),
          let value = String(data: data, encoding: .utf8),
          Data(value.utf8) == data
    else { return .invalid }
    return .value(value)
}

private func publishCreateOnce(data: Data, destinationPath: String) -> Bool {
    let temporaryPath = destinationPath + "." + UUID().uuidString.lowercased() + ".tmp"
    let temporaryURL = URL(fileURLWithPath: temporaryPath)
    do {
        try data.write(to: temporaryURL, options: .withoutOverwriting)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o600],
            ofItemAtPath: temporaryPath
        )
        let descriptor = open(temporaryPath, O_RDONLY | O_NOFOLLOW | O_NONBLOCK)
        guard descriptor >= 0 else { throw NSError(domain: "AppShareHandoff", code: 2) }
        var descriptorBefore = stat()
        var pathBefore = stat()
        let bound = fstat(descriptor, &descriptorBefore) == 0 &&
            lstat(temporaryPath, &pathBefore) == 0 &&
            ownerPrivateOrdinaryFile(descriptorBefore, maximumBytes: 16 * 1024) &&
            Int64(data.count) == Int64(descriptorBefore.st_size) &&
            sameStableFileIdentity(descriptorBefore, pathBefore)
        let synced = bound && fsync(descriptor) == 0
        var descriptorAfter = stat()
        var pathAfter = stat()
        let stable = synced && fstat(descriptor, &descriptorAfter) == 0 &&
            lstat(temporaryPath, &pathAfter) == 0 &&
            sameStableFileIdentity(descriptorBefore, descriptorAfter) &&
            sameStableFileIdentity(descriptorAfter, pathAfter)
        close(descriptor)
        guard stable else { throw NSError(domain: "AppShareHandoff", code: 3) }

        let renamed = temporaryPath.withCString { source in
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
        guard renamed,
              secureSha256(path: destinationPath) == sha256(data)
        else { throw NSError(domain: "AppShareHandoff", code: 4) }

        let directoryPath = (destinationPath as NSString).deletingLastPathComponent
        let directoryDescriptor = open(directoryPath, O_RDONLY)
        guard directoryDescriptor >= 0 else { throw NSError(domain: "AppShareHandoff", code: 5) }
        let directorySynced = fsync(directoryDescriptor) == 0
        close(directoryDescriptor)
        return directorySynced
    } catch {
        try? FileManager.default.removeItem(atPath: temporaryPath)
        return false
    }
}

private func parseControl(_ raw: String) -> ControlRecord? {
    let lines = raw.split(separator: "\n", omittingEmptySubsequences: false).map(String.init)
    guard lines.last == "" else { return nil }
    switch Array(lines.dropLast()) {
    case let values where values.count == 2 && values[0] == HandoffState.ready.rawValue:
        guard isCanonicalSha256(values[1]) else { return nil }
        return ControlRecord(
            state: .ready,
            requestSha256: values[1],
            startReceiptSha256: nil,
            productActionStartedAt: nil,
            productActionCompletedAt: nil
        )
    case let values where values.count == 4 && values[0] == HandoffState.action.rawValue:
        guard isCanonicalSha256(values[1]), isCanonicalSha256(values[2]),
              parseCanonicalTimestamp(values[3]) != nil
        else {
            return nil
        }
        return ControlRecord(
            state: .action,
            requestSha256: values[1],
            startReceiptSha256: values[2],
            productActionStartedAt: values[3],
            productActionCompletedAt: nil
        )
    case let values where values.count == 5 && values[0] == HandoffState.complete.rawValue:
        guard isCanonicalSha256(values[1]), isCanonicalSha256(values[2]),
              let startedAt = parseCanonicalTimestamp(values[3]),
              let completedAt = parseCanonicalTimestamp(values[4]),
              startedAt <= completedAt
        else { return nil }
        return ControlRecord(
            state: .complete,
            requestSha256: values[1],
            startReceiptSha256: values[2],
            productActionStartedAt: values[3],
            productActionCompletedAt: values[4]
        )
    default:
        return nil
    }
}

private final class HandoffController: NSObject, NSApplicationDelegate {
    private let controlPath: String
    private let requestPath: String
    private let startReceiptPath: String
    private let completeReceiptPath: String
    private let requestId: String
    private let armExpiresAt: Date
    private let hardExpiresAt: Date
    private let panel: NonactivatingHandoffPanel
    private let heading = NSTextField(labelWithString: "")
    private let instruction = NSTextField(wrappingLabelWithString: "")
    private let actionButton = NSButton(title: "", target: nil, action: nil)
    private let countdown = NSTextField(labelWithString: "")
    private var state = HandoffState.waiting
    private var requestSha256: String?
    private var startReceiptSha256: String?
    private var startReceiptCreatedAt: Date?
    private var productActionStartedAt: String?
    private var productActionCompletedAt: String?
    private var actionExpiresAt: Date?
    private var timer: Timer?

    init(
        controlPath: String,
        requestPath: String,
        startReceiptPath: String,
        completeReceiptPath: String,
        requestId: String,
        armExpiresAt: Date,
        hardExpiresAt: Date
    ) {
        self.controlPath = controlPath
        self.requestPath = requestPath
        self.startReceiptPath = startReceiptPath
        self.completeReceiptPath = completeReceiptPath
        self.requestId = requestId
        self.armExpiresAt = armExpiresAt
        self.hardExpiresAt = hardExpiresAt
        panel = NonactivatingHandoffPanel(
            contentRect: NSRect(x: 0, y: 0, width: 560, height: 245),
            styleMask: [.titled, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        panel.title = stableWindowTitle
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        panel.hidesOnDeactivate = false
        panel.ignoresMouseEvents = false
        panel.acceptsMouseMovedEvents = false
        panel.becomesKeyOnlyIfNeeded = true
        panel.hasShadow = true
        panel.isOpaque = true
        panel.isReleasedWhenClosed = false
        panel.standardWindowButton(.closeButton)?.isHidden = true
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true
        panel.setAccessibilityTitle(stableWindowTitle)
        panel.setAccessibilityLabel("Local Browser Bridge exact app-share handoff")

        let content = NSView(frame: panel.contentView!.bounds)
        content.autoresizingMask = [.width, .height]
        content.wantsLayer = true
        panel.contentView = content

        heading.frame = NSRect(x: 24, y: 172, width: 512, height: 38)
        heading.font = NSFont.systemFont(ofSize: 24, weight: .bold)
        heading.alignment = .center
        heading.textColor = .white
        heading.isSelectable = false
        heading.setAccessibilityLabel("App-share handoff state")
        content.addSubview(heading)

        instruction.frame = NSRect(x: 34, y: 112, width: 492, height: 54)
        instruction.font = NSFont.systemFont(ofSize: 15, weight: .semibold)
        instruction.alignment = .center
        instruction.textColor = .white
        instruction.maximumNumberOfLines = 2
        instruction.isSelectable = false
        instruction.setAccessibilityLabel("App-share handoff instruction")
        content.addSubview(instruction)

        actionButton.frame = NSRect(x: 100, y: 57, width: 360, height: 44)
        actionButton.bezelStyle = .rounded
        actionButton.font = NSFont.systemFont(ofSize: 16, weight: .bold)
        actionButton.target = self
        actionButton.action = #selector(receiveAppShareAction)
        actionButton.setAccessibilityLabel(readyButtonTitle)
        actionButton.setAccessibilityIdentifier("lbb-app-share-start")
        content.addSubview(actionButton)

        countdown.frame = NSRect(x: 24, y: 18, width: 512, height: 26)
        countdown.font = NSFont.monospacedDigitSystemFont(ofSize: 14, weight: .medium)
        countdown.alignment = .center
        countdown.textColor = .white
        countdown.isSelectable = false
        countdown.setAccessibilityLabel("App-share handoff deadline")
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

    @objc private func receiveAppShareAction() {
        guard state == .ready, actionButton.isEnabled, let requestSha256,
              secureSha256(path: requestPath) == requestSha256
        else { return }
        actionButton.isEnabled = false
        let receiptCreatedAtText = canonicalTimestamp()
        guard let receiptCreatedAt = parseCanonicalTimestamp(receiptCreatedAtText) else {
            NSApp.terminate(nil)
            return
        }
        let receipt: [String: Any] = [
            "acceptedAsAuthority": false,
            "buttonAccepted": true,
            "buttonActionObserved": true,
            "cryptographicToolIdentityClaimed": false,
            "createdAt": receiptCreatedAtText,
            "kind": "macos-app-share-concurrency-handoff-start",
            "physicalHumanProvenanceClaimed": false,
            "productVersion": productVersion,
            "promptPid": Int(getpid()),
            "requestId": requestId,
            "requestSha256": requestSha256,
            "schemaVersion": 2,
        ]
        do {
            let receiptData = try canonicalJSON(receipt)
            let receiptSha256 = sha256(receiptData)
            guard publishCreateOnce(
                data: receiptData,
                destinationPath: startReceiptPath
            ) else {
                throw NSError(domain: "AppShareHandoff", code: 6)
            }
            startReceiptSha256 = receiptSha256
            startReceiptCreatedAt = receiptCreatedAt
            updatePresentation(.armed)
        } catch {
            fputs("App-share start receipt publication failed.\n", stderr)
            timer?.invalidate()
            NSApp.terminate(nil)
        }
    }

    private func pollRunnerState() {
        let expiration = state == .waiting || state == .ready || state == .armed
            ? armExpiresAt
            : actionExpiresAt ?? hardExpiresAt
        guard expiration.timeIntervalSinceNow > 0 else {
            timer?.invalidate()
            NSApp.terminate(nil)
            return
        }

        switch secureReadText(path: controlPath, maximumBytes: 4 * 1024) {
        case .missing, .changed:
            break
        case .invalid:
            timer?.invalidate()
            fputs("App-share handoff control state is unreadable or invalid.\n", stderr)
            NSApp.terminate(nil)
            return
        case let .value(raw):
            do {
                guard let control = parseControl(raw) else {
                    throw NSError(domain: "AppShareHandoff", code: 7)
                }
                try apply(control)
            } catch {
                timer?.invalidate()
                fputs("App-share handoff control state is unreadable or invalid.\n", stderr)
                NSApp.terminate(nil)
                return
            }
        }

        let updatedExpiration = state == .waiting || state == .ready || state == .armed
            ? armExpiresAt
            : actionExpiresAt ?? hardExpiresAt
        let remaining = max(0, Int(updatedExpiration.timeIntervalSinceNow.rounded(.up)))
        countdown.stringValue = state == .complete
            ? "Closing in: \(remaining)s"
            : "Time remaining: \(remaining)s"
    }

    private func apply(_ control: ControlRecord) throws {
        switch (state, control.state) {
        case (.waiting, .ready):
            requestSha256 = control.requestSha256
            updatePresentation(.ready)
        case (.ready, .ready), (.armed, .ready):
            guard requestSha256 == control.requestSha256 else {
                throw NSError(domain: "AppShareHandoff", code: 8)
            }
        case (.armed, .action):
            guard requestSha256 == control.requestSha256,
                  let receiptSha = control.startReceiptSha256,
                  let startedAt = control.productActionStartedAt,
                  startReceiptSha256 == receiptSha,
                  let receiptCreatedAt = startReceiptCreatedAt,
                  let startedAtDate = parseCanonicalTimestamp(startedAt),
                  startedAtDate >= receiptCreatedAt,
                  startedAtDate.timeIntervalSince(receiptCreatedAt) <= completionGraceSeconds,
                  startedAtDate.timeIntervalSinceNow <= 1
            else { throw NSError(domain: "AppShareHandoff", code: 9) }
            productActionStartedAt = startedAt
            actionExpiresAt = min(
                hardExpiresAt,
                receiptCreatedAt.addingTimeInterval(completionGraceSeconds)
            )
            updatePresentation(.action)
            countdown.stringValue = "Product action started: \(startedAt)"
        case (.action, .action):
            guard requestSha256 == control.requestSha256,
                  startReceiptSha256 == control.startReceiptSha256,
                  productActionStartedAt == control.productActionStartedAt
            else { throw NSError(domain: "AppShareHandoff", code: 10) }
        case (.action, .complete):
            guard requestSha256 == control.requestSha256,
                  startReceiptSha256 == control.startReceiptSha256,
                  let receiptSha = control.startReceiptSha256,
                  let startedAt = control.productActionStartedAt,
                  let completedAt = control.productActionCompletedAt,
                  productActionStartedAt == startedAt,
                  let receiptCreatedAt = startReceiptCreatedAt,
                  let startedAtDate = parseCanonicalTimestamp(startedAt),
                  let completedAtDate = parseCanonicalTimestamp(completedAt),
                  completedAtDate >= startedAtDate,
                  completedAtDate.timeIntervalSince(startedAtDate) <= completionGraceSeconds,
                  completedAtDate.timeIntervalSince(receiptCreatedAt) <= completionGraceSeconds,
                  completedAtDate.timeIntervalSinceNow <= 1
            else { throw NSError(domain: "AppShareHandoff", code: 11) }
            let completionCreatedAt = canonicalTimestamp()
            guard let completionCreatedAtDate = parseCanonicalTimestamp(completionCreatedAt),
                  completionCreatedAtDate >= completedAtDate,
                  completionCreatedAtDate.timeIntervalSince(receiptCreatedAt) <=
                    completionGraceSeconds
            else { throw NSError(domain: "AppShareHandoff", code: 11) }
            let completion: [String: Any] = [
                "acceptedAsAuthority": false,
                "buttonRemainedDisabledDuringProductAction": true,
                "createdAt": completionCreatedAt,
                "cryptographicToolIdentityClaimed": false,
                "handoffStateSequenceBound": true,
                "kind": "macos-app-share-concurrency-handoff-complete",
                "physicalHumanProvenanceClaimed": false,
                "productActionCompletedAt": completedAt,
                "productActionStartedAt": startedAt,
                "productVersion": productVersion,
                "promptPid": Int(getpid()),
                "requestId": requestId,
                "requestSha256": control.requestSha256,
                "schemaVersion": 2,
                "startReceiptSha256": receiptSha,
            ]
            guard publishCreateOnce(
                data: try canonicalJSON(completion),
                destinationPath: completeReceiptPath
            ) else { throw NSError(domain: "AppShareHandoff", code: 12) }
            productActionCompletedAt = completedAt
            updatePresentation(.complete)
        case (.complete, .complete):
            guard requestSha256 == control.requestSha256,
                  startReceiptSha256 == control.startReceiptSha256,
                  productActionStartedAt == control.productActionStartedAt,
                  productActionCompletedAt == control.productActionCompletedAt
            else { throw NSError(domain: "AppShareHandoff", code: 13) }
        default:
            throw NSError(domain: "AppShareHandoff", code: 13)
        }
    }

    private func updatePresentation(_ next: HandoffState) {
        state = next
        let color = next.background
        panel.backgroundColor = color
        panel.contentView?.layer?.backgroundColor = color.cgColor
        heading.stringValue = next.heading
        instruction.stringValue = next.instruction
        switch next {
        case .waiting:
            actionButton.title = "WAITING FOR RUNNER"
            actionButton.isEnabled = false
        case .ready:
            actionButton.title = readyButtonTitle
            actionButton.isEnabled = true
        case .armed:
            actionButton.title = armedButtonTitle
            actionButton.isEnabled = false
        case .action:
            actionButton.title = actionButtonTitle
            actionButton.isEnabled = false
        case .complete:
            actionButton.title = completeButtonTitle
            actionButton.isEnabled = false
        }
        actionButton.setAccessibilityLabel(actionButton.title)
    }
}

private func runSelfTest() {
    precondition(HandoffState.allCases.map(\.rawValue) == ["WAITING", "READY", "ARMED", "ACTION", "COMPLETE"])
    precondition(stableWindowTitle == "LBB macOS Acceptance App Share")
    precondition(readyButtonTitle == "START APP-SHARE CHECK")
    precondition(parseControl("READY\n" + String(repeating: "a", count: 64) + "\n")?.state == .ready)
    precondition(parseControl("READY\ninvalid\n") == nil)
    let requestHash = String(repeating: "a", count: 64)
    let startHash = String(repeating: "b", count: 64)
    let startedAt = "2026-08-24T00:00:00.000Z"
    let completedAt = "2026-08-24T00:00:00.100Z"
    precondition(
        parseControl("ACTION\n\(requestHash)\n\(startHash)\n\(startedAt)\n")?.state == .action
    )
    precondition(
        parseControl(
            "COMPLETE\n\(requestHash)\n\(startHash)\n\(startedAt)\n\(completedAt)\n"
        )?.state == .complete
    )
    precondition(parseControl("ACTION\n\(requestHash)\n\(startHash)\nnot-a-time\n") == nil)
    precondition(
        parseControl(
            "COMPLETE\n\(requestHash)\n\(startHash)\n\(completedAt)\n\(startedAt)\n"
        ) == nil
    )
    let timestamp = canonicalTimestamp()
    precondition(parseCanonicalTimestamp(timestamp) != nil)
    precondition(parseCanonicalTimestamp("2026-08-24T00:00:00Z") == nil)
    let deadlinePolicyNow = Date(timeIntervalSince1970: 2_000_000_000)
    let acceptedArmExpiration = deadlinePolicyNow.addingTimeInterval(armWindowSeconds)
    let acceptedHardExpiration = acceptedArmExpiration.addingTimeInterval(
        completionGraceSeconds
    )
    let acceptedInvocation = validateHandoffInvocation(
        armExpirationMilliseconds: acceptedArmExpiration.timeIntervalSince1970 * 1_000,
        hardExpirationMilliseconds: acceptedHardExpiration.timeIntervalSince1970 * 1_000,
        now: deadlinePolicyNow
    )
    guard let acceptedInvocation else {
        preconditionFailure("the exact 300s arm plus 18s completion invocation was refused")
    }
    precondition(
        acceptedInvocation.hardExpiration.timeIntervalSince(
            acceptedInvocation.armExpiration
        ) == completionGraceSeconds
    )
    precondition(validateHandoffInvocation(
        armExpirationMilliseconds: acceptedArmExpiration.timeIntervalSince1970 * 1_000,
        hardExpirationMilliseconds:
            acceptedHardExpiration.addingTimeInterval(0.001).timeIntervalSince1970 * 1_000,
        now: deadlinePolicyNow
    ) == nil, "300s arm and 18s completion accepted; beyond-policy invocation refused")
    precondition(validateHandoffInvocation(
        armExpirationMilliseconds:
            acceptedArmExpiration.addingTimeInterval(0.001).timeIntervalSince1970 * 1_000,
        hardExpirationMilliseconds: acceptedHardExpiration.timeIntervalSince1970 * 1_000,
        now: deadlinePolicyNow
    ) == nil)
    let testDirectory = FileManager.default.temporaryDirectory
        .appendingPathComponent("lbb-app-share-handoff-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: testDirectory) }
    do {
        try FileManager.default.createDirectory(
            at: testDirectory,
            withIntermediateDirectories: false,
            attributes: [.posixPermissions: 0o700]
        )
        let destination = testDirectory.appendingPathComponent("published").path
        let first = try canonicalJSON(["value": "first"])
        let second = try canonicalJSON(["value": "second"])
        precondition(publishCreateOnce(data: first, destinationPath: destination))
        precondition(!publishCreateOnce(data: second, destinationPath: destination))
        let published = try Data(contentsOf: URL(fileURLWithPath: destination))
        precondition(published == first)
        let expectedHash = sha256(first)
        precondition(secureSha256(path: destination) == expectedHash)
        guard case let .value(readBack) = secureReadText(path: destination, maximumBytes: 16 * 1024)
        else { preconditionFailure("stable text read rejected its own ordinary file") }
        precondition(Data(readBack.utf8) == first)

        let symlinkPath = testDirectory.appendingPathComponent("symlink").path
        precondition(symlink(destination, symlinkPath) == 0)
        guard case .invalid = secureReadText(path: symlinkPath, maximumBytes: 16 * 1024)
        else { preconditionFailure("stable text read followed a symlink") }

        let hardlinkPath = testDirectory.appendingPathComponent("hardlink").path
        precondition(link(destination, hardlinkPath) == 0)
        guard case .invalid = secureReadText(path: destination, maximumBytes: 16 * 1024)
        else { preconditionFailure("stable text read accepted a multiply linked file") }
        try FileManager.default.removeItem(atPath: hardlinkPath)

        let fifoPath = testDirectory.appendingPathComponent("fifo").path
        precondition(mkfifo(fifoPath, 0o600) == 0)
        guard case .invalid = secureReadText(path: fifoPath, maximumBytes: 16 * 1024)
        else { preconditionFailure("stable text read accepted a FIFO") }
    } catch {
        preconditionFailure("app-share handoff filesystem self-test failed")
    }
    print("macOS app-share handoff self-test passed")
}

let arguments = Array(CommandLine.arguments.dropFirst())
if arguments == ["--self-test"] {
    runSelfTest()
    exit(0)
}
guard arguments.count == 7,
      arguments[0].hasPrefix("/"),
      arguments[1].hasPrefix("/"),
      arguments[2].hasPrefix("/"),
      arguments[3].hasPrefix("/"),
      isCanonicalRequestId(arguments[4]),
      let armExpirationMilliseconds = Double(arguments[5]),
      let hardExpirationMilliseconds = Double(arguments[6])
else {
    fputs("Usage: app-share-handoff <control> <start-receipt> <complete-receipt> <request-marker> <request-id> <arm-expiration-epoch-ms> <hard-expiration-epoch-ms>\n", stderr)
    exit(2)
}

guard let invocation = validateHandoffInvocation(
    armExpirationMilliseconds: armExpirationMilliseconds,
    hardExpirationMilliseconds: hardExpirationMilliseconds
) else {
    fputs("App-share handoff deadlines are outside the accepted windows.\n", stderr)
    exit(2)
}

let application = NSApplication.shared
application.setActivationPolicy(.accessory)
private let controller = HandoffController(
    controlPath: arguments[0],
    requestPath: arguments[3],
    startReceiptPath: arguments[1],
    completeReceiptPath: arguments[2],
    requestId: arguments[4],
    armExpiresAt: invocation.armExpiration,
    hardExpiresAt: invocation.hardExpiration
)
application.delegate = controller
application.run()
