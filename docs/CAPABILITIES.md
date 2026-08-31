# Capabilities

This page states what the current code can do, what the user must configure, and what still lacks release-grade evidence. A capability is not a security boundary: browser control uses the selected signed-in profile, and native control uses the selected window in the user's current login session.

## Status terms

- **Available** means the implementation and protocol exist.
- **Prerequisite** means the feature is available only after the listed browser or operating-system setup.
- **Evidence gap** means the code exists but the exact platform, application, or packaged-release path has not yet been proven by the checked-in live evidence.
- **Not included** means clients must not infer the capability from a nearby feature.

## Browser control

| Capability | Status | Requirement or boundary |
|---|---|---|
| List, activate, create at a policy-approved URL, navigate, reload, and close tabs | Available | Chrome or Edge 140+; omit the creation URL only for a blank lifecycle tab |
| Observe viewport pixels, text, selection, and interactive elements | Available | Regular HTTP(S) pages; file pages require the browser's file-URL permission |
| Click, hover, fill, select, scroll, type, and send key chords | Available | One explicit debugger-backed lease on one tab |
| Browser key subset | Available | Documented CDP named keys, F1–F12, ASCII letters/digits, and one non-control/non-whitespace BMP scalar other than the reserved `+`; Safe mode is narrower |
| Main-world JavaScript evaluation | Available | Full Access, or the applicable Safe-mode policy |
| Same-process frame observation | Available | Chrome or Edge 140+ |
| Recursive cross-origin iframe observation and trusted point input | Available | Chrome or Edge 140+; bounded to 16 iframe targets and five levels |
| Dialog handling, condition waits, and bounded action batches | Available | Commands remain tied to the current lease and observation epoch |
| Browser-owned warning and extension-owned page indicator | Available | Chrome owns the authoritative warning/Cancel; the page pill combines a direct-root-child/innermost-host-bound private marker, initial/final host/root accessibility checks, five browser-process point hits, bounded top-layer ancestry, separate top-layer revision/content-loss generations, a 500 ms sampling attempt, and an absolute 3 s dirty-proof deadline |
| Full Access | Available and default | Broad authority over regular pages in the selected profile |
| Safe mode | Available | Site allowlist, sensitive-field blocking, and selected one-time approvals |
| Per-command API cancellation | Available | Bearer-authenticated in-flight `callId`; returns outcome-unknown and requires observation, never automatic retry |
| Saved-token removal | Available | Trusted popup action revokes control, disconnects, removes the token from extension storage, and verifies the cleared state |

The extension never replaces trusted debugger input with an untrusted page-generated click. Chrome Cancel, the in-page **Stop** button, popup release, lease expiry, tab closure, or connector loss revokes control.

Canceling one bearer API command stops only that command context and preserves the user's browser-control lease when it can be kept safely. Cancellation is cooperative and can race a dispatched side effect, so the original command is completed as outcome-unknown rather than reported as rolled back. A controlled-page command with an unknown outcome immediately clears the server's observation/screenshot and latches exact-session recovery; later page mutations are refused until an explicit `page.observe` succeeds, even if the extension never receives the cancel. This also covers a disconnected caller with or without `callId`, legacy dashboard actions, and connector timeouts. The extension advances and persists the lease turn before its next queued command, re-clears frame state at the queue barrier, and revokes the lease on persistence failure.

Every primary browser command and follow-up tab list or page observation is bound to the exact extension-session UUID selected before dispatch. A replacement extension never receives work that belonged to the old session.

## Agent and shell access

| Capability | Status | Requirement or boundary |
|---|---|---|
| Bearer JSON command API | Available | Loopback plus master token; POST body |
| Agent Fetch GET-only API | Available | Private derived capability URL; stable `callId` required for actions |
| Native shell status | Available | No shell authority required |
| Windows PowerShell and cmd | Available, opt-in | Server started with `--enable-shell` or `LBB_ENABLE_SHELL=1` |
| macOS zsh and sh | Available, opt-in | Same explicit shell grant |

Agent Fetch shares the POST command dispatcher, replay cache, cancellation,
and fail-closed unknown-outcome rules. Shell commands are non-interactive and
bounded, but the capability is full current-user code execution and is not
confined by the browser or exact-window control model.

## Native application control

The optional helper is a separate Rust process. It connects outbound to the loopback server and exposes a fixed computer-use command set; it does not expose shell, filesystem, clipboard, process-launch, download, or telemetry methods.

| Capability | macOS | Windows |
|---|---|---|
| One-shot exact-window observation | Available through the exact-window snapshot backend | Starts the same bounded, exact-HWND Windows Graphics Capture backend used by live sharing and stops it after one fresh frame |
| Persistent exact-window live share | ScreenCaptureKit `SCStream` bound to an exact `(PID, CGWindowID)` | Project-owned Windows Graphics Capture `CreateFreeThreaded` frame pool on a dedicated MTA owner thread, bound to an exact `(PID, HWND)` |
| Live-share transport | Requested 1–10 FPS maximum cadence, PNG frames capped at 1,000,000 pixels, monotonic sequence, bounded latest-frame slot, optional acknowledgement pacing | Same protocol and bounds |
| Captured cursor | System cursor disabled; helper cursor composited into returned frames | Same |
| Semantic observation/action | macOS Accessibility | Windows UI Automation |
| Background pixel/key route | Process/window-targeted route using dynamically resolved macOS facilities | UIA plus exact-HWND background messages where the application accepts them |
| Route provenance | Every mutation records a sealed exact-target `inputDelivery`; private SkyLight routes are labelled `privateUnsupported` | Every mutation records the UIA or exact-HWND route and whether an API acceptance signal existed |
| Target-effect proof | Accessibility read-back or another allowlisted target postcondition is required for `Confirmed` | UI Automation read-back or another allowlisted target postcondition is required for `Confirmed` |
| Native key subset | Navigation/editing keys, F1–F12, ASCII letters/digits, mapped US-keyboard punctuation, and Control/Alt/Shift/Meta modifiers | Navigation/editing keys, F1–F12, ASCII letters/digits, mapped punctuation, and Control/Alt/Shift; Windows/global and secure chords fail closed |
| Readiness signal | Current permission and complete focus/input snapshot must be readable | Non-Session-0 process plus readable input desktop, foreground/focus HWNDs, and cursor; provider acceptance is still per action |
| Target-activation disclosure | Target-routed pointer paths, including move trajectories, and native text may use and restore a transient exact-target `AXFrontmost` lease plus a target-only make-key commit; no `AXRaise`, OS-front-process switch, or Space switch | No explicit target-activation or foreground API; provider behavior is still checked before/after |
| Foreground/focus invariant | WindowServer-front process/window and saved user AX state must match before/after; no zero-transient guarantee | Foreground and GUI-thread focus HWNDs must match before/after; no zero-transient guarantee |
| Shared pointer attribution | Global position is diagnostic; a healthy HID-system boundary covering movement, drag, buttons, scroll, and tablet activity can distinguish a quiet interval from shared-session contamination, without claiming physical-device provenance | Global position is diagnostic; message-only Raw Input and a minimal low-level injected-flag epoch retain counters/health only. They do not identify a physical actor and can have integrity, remote, or virtual-input blind spots |
| Helper transport lifecycle | Intentional or unexpected server-transport loss exits the helper; relaunch is required. Explicit share stop stays in process | The launcher supervises disposable workers and restarts them after transport loss. Explicit share stop stays in process |
| Automatic foreground fallback | Not included | Not included |
| Helper-requested physical pointer movement or global HID input | Not included; source and packaged-artifact audits also forbid known global cursor/HID APIs | Not included |

The live-share target is selected in the Local Browser Bridge control page. The helper starts the native stream programmatically for that exact process/window pair. It does **not** present `SCContentSharingPicker` on macOS or the Windows system capture picker.

The operating system still owns capture lifecycle UI. macOS can show its current screen-capture indicator and stop affordance; Windows can show its normal WGC capture border or indicator. Their exact appearance varies by OS version and policy, so the project does not promise a particular banner or wording.

Every primary computer command and follow-up observation is bound to the exact helper-session UUID selected before dispatch. Share start is accepted only from a raw `{ "active": true, "id": "..." }` result followed by a first observation carrying that exact ID; share stop requires raw `{ "active": false }`. Rejected lifecycle results are quarantined and cleaned up by an exact-session task that survives caller cancellation. If stop cannot be proven, the server revokes only the originating WebSocket through a queue-independent shutdown signal; a replacement helper is never used for that cleanup.

The helper advertises `computer.input-delivery-provenance.v1` and `computer.pointer-activity-monitor.v1` for the layered action result and bounded activity monitor. These are negotiated metadata, not callable methods. Pointer diagnostics retain no device IDs, coordinates, or input contents.

## Setup prerequisites

These are setup conditions, not product limitations:

- Every installed component must use the same release version.
- Browser control requires an unpacked Manifest V3 extension loaded from `chrome://extensions` or `edge://extensions`.
- Browser control requires Chromium 140+. Cross-origin child-session routing first appeared in Chromium 125, but version 0.12.67 retains the overall floor so persisted extension storage can be restricted to trusted contexts.
- The complete macOS archive requires macOS 13+. Live sharing additionally requires Screen Recording permission for the packaged helper application.
- macOS semantic control and supported input routes require Accessibility permission.
- Windows native control must run in the signed-in interactive session, not Session 0 or a service.
- The selected native window must be on screen, non-minimized, and have a nonzero capturable area when sharing starts.

See [Installation](INSTALL.md) for the user flow.

## Not included

- A native content picker or OS-managed per-window consent dialog
- Picture-in-Picture automation on a second desktop
- A virtual display, VM, RDP loopback, separate login, or separate OS input seat
- Independent simultaneous input inside the same login session; current native control is cooperative shared-session operation
- Security isolation from the user's current account, credentials, applications, clipboard, or files
- Guaranteed background input for every application framework
- Audio capture or video/WebRTC transport
- A native desktop cursor overlay; the helper cursor appears in returned images

## Evidence status

- The browser evidence under [`evidence/v0.11.1`](../evidence/v0.11.1/README.md) separates published 0.11.1 results from a local 0.11.2 recursive-frame candidate.
- The exact v0.12.8 macOS candidate passed 187/187 persistent-stream checks and produced six reviewed screenshots. Its same-candidate Windows run delivered a fresh foreground-arm request but received no click and created no received marker; it timed out at `wait-foreground-arm` before the invariant baseline or any product action. Chrome acceptance never started, publication was canceled, and no v0.12.8 Release exists.
- The exact v0.12.9 candidate was withdrawn after its single packaged macOS run observed a global cursor-position delta at the first semantic `setValue`. Forty precondition assertions had passed and one exact-window screenshot was retained, but the old two-coordinate invariant could not identify the mover. Windows and stock-Chrome acceptance were not started, and no v0.12.9 Release exists.
- The exact v0.12.10 candidate passed 69 packaged macOS assertions and retained six reviewed fixture-only screenshots, then timed out before its post-resize action because no separately authorized shared-pointer movement reached the independent pre-action probe. The one-shot candidate was not retried; Windows and stock-Chrome acceptance were not started, publication was canceled, and no v0.12.10 Release exists.
- The exact v0.12.11 workflow candidate passed its source, build, checksum, format, and GitHub provenance gates, but its receipt exposed only one macOS result digest while release policy required two fresh, non-mergeable lanes. It was withdrawn before any candidate byte ran; no Windows or stock-Chrome test started and no Release exists.
- The exact v0.12.12 candidate was withdrawn after three macOS deliberate-concurrency attempts stopped before product dispatch: one at the arm deadline and two on pre-dispatch pointer/input contamination. Windows and stock-Chrome did not complete, publication remained blocked, and no Release exists.
- The exact v0.12.13 candidate was withdrawn after its quiet macOS lane passed 192 of 193 assertions but the final unchanged whole-run SystemProbe boundary observed unrelated shared-seat `mouseMoved`/cursor activity. Deliberate macOS, Windows, and stock-Chrome were not started, publication was canceled, and no Release exists.
- The exact v0.12.22 candidate passed its quiet-seat preflight and returned a Confirmed semantic action with safe sealed route, focus, pointer, and Space fields, but the harness applied its keyboard-aware independent classifier to that pointer-only action schema and stopped after 55 of 56 checks. Deliberate macOS, Windows, and stock-Chrome were not started, publication was canceled, and no Release exists.
- Version 0.12.23 retained the exact app-share surface introduced in v0.12.22 and separated the sealed action-pointer classifier from the keyboard-aware independent-system classifier. Its quiet packaged lane passed 208/208 checks. Its deliberate lane accepted the exact app-share start receipt and completed 89/89 recorded assertions, but then reused a pre-handoff stream frame after 43.807 seconds; `computer.click` correctly returned HTTP 409 `COMPUTER_STALE_FRAME` before dispatch. No completion receipt, Windows, stock-Chrome, publication, or Release followed. The exact ten-file negative record is retained on the evidence-only branch [`evidence/v0.12.23-macos-app-share-stale-frame-32746618027`](https://github.com/flrngel/local-browser-bridge/tree/4e4db75a4ede915d982d139a82dacac8a6c4772a/evidence/v0.12.23/computer/attempts/withdrawn-9e50811-macos-app-share-stale-frame).
- Version 0.12.24 added the bounded post-`ACTION` successor-frame refresh, then its exact Windows read-only handoff watcher failed before operator action because a closure-created PowerShell 5.1 dynamic module could not resolve the atomic marker reader. Stock-Chrome and publication did not follow.
- Version 0.12.25 retained that authority refresh and made the Windows read-only watcher portable to exact system PowerShell 5.1 by passing marker paths as explicit callback arguments, but its terminal-hosted sentinel was not discoverable through the real Windows app-share. Version 0.12.26 builds the exact trusted fixture source as an ephemeral `WindowsApplication`, binds its source/executable hashes and exact process/session/PID topology, and removes it after the run. The stable AUMID improves app discovery only; it is not consent or control authority.
- Version 0.12.26 attempt 1 bound and ran only the macOS quiet lane, which failed closed when the independent monitor observed shared-seat HID pointer activity during `computer.typeText`; attempt 2 rebuilt but stopped before candidate execution when the verifier rejected the byte-identical extension's valid same-run attestations from two attempts. Both negative records were preserved and both attempts were canceled; neither reached Windows, stock Chrome, or a public Release.
- Version 0.12.27 added strict exact-attempt attestation selection and atomic five-file local deployment. It passed both packaged macOS lanes, but its ad-hoc Windows launcher lost the runner's terminal streams and exit fact before an evidence directory appeared; candidate-byte execution could not be proved or excluded, so the version was withdrawn without a Windows action, Chrome run, or Release. Version 0.12.28 was the following blocked source checkpoint, not a candidate. Version 0.12.29 completed the coordinator source gate without changing product, protocol, or platform capabilities. Its checked-in Windows coordinator uses flush-before-move create-once state, an owner-private allowlisted worker environment, kill-on-close process-tree containment, persistent separate stream files, exact worker/runner PID-plus-start-time binding, and complete predecessor-chain validation. Under the stable admission mutex, one monotonic deadline covers exact prior-Job termination, the zero-active query, namespace disappearance, and one final create. A returned handle is accepted only with last error zero; every nonzero status, including `ERROR_ALREADY_EXISTS`, is closed and refused without adoption or retry. The fresh Job is configured and the worker bound before Worker or Intent publication. All eight GUID-scoped native SelfTest scenarios passed under exact 64-bit system Windows PowerShell 5.1, and independent review found no P0/P1 issue. The read-only watcher still starts only after the atomic foreground-arm request marker exists. Every `Follow` result remains notification-only with `uiActionAllowed: false` and cannot grant consent or action authority; every started or uncertain attempt is terminal with no retry. No 0.12.29 packaged candidate was built, downloaded, or executed, and no macOS, Windows-candidate, stock-Chrome, tag, evidence-publication, or Release gate ran for it. Version 0.12.30 added the tagless schema-3 candidate/publication split and produced one candidate whose two macOS lanes passed; Windows then failed closed before runner launch because the staged worker-support loader referenced an undefined hex helper. Version 0.12.31 fixed that loader and passed its quiet macOS lane, but its deliberate lane failed closed before product dispatch when the bounded post-`ACTION` fresh-share-frame refresh timed out. Version 0.12.32 introduced the larger bounded, abortable authority wait and built one trusted candidate; its source-compiled app-share self-test then failed on an extra stdout line before permissions, quiet-seat stabilization, or candidate process launch. Windows candidate execution, stock Chrome, tagging, publication, and a public Release did not follow any of those failures. Version 0.12.33 retained the same product and release boundaries, removed the stray diagnostic line, and made CI, candidate packaging, and the packaged rig enforce the same byte-exact self-test result. Its two fresh macOS lanes passed, but its single Windows attempt failed closed at `build-dedicated-fixture` before any product binary or Chrome action. Version 0.12.34 enabled explicit child breakaway and atomic private Job-list creation and added a nested fixture-build self-test before candidate construction. Its trust gate and both macOS lanes passed, but Windows was interrupted after the old pre-coordinator persistent reservation and the version was withdrawn without retry. Version 0.12.37 retains that Job topology and moves a schema-2 reservation bound to an opaque coordinator instance into the worker's last pre-intent boundary. The coordinator assumes an independently verified GitHub-attested candidate is trusted and is not a hostile-code sandbox.
- Persistent native-stream code still needs packaged, version-specific live proof on both operating systems before its implementation status is treated as release proof.
- Cross-Space macOS capture/input, minimized-window capture, protected content, elevated Windows targets, and a broad application compatibility matrix remain unproven and are not advertised.

Transport success is diagnostic evidence. A native action is supported only when its exact-target route is sealed, any API acceptance is labelled only as such, a representative application-owned result is observed when confirmation is claimed, and the advertised platform-specific foreground/window-focus, pointer-attribution, and desktop invariants hold. A contaminated shared-pointer interval can reflect unrelated activity without identifying the helper as its source. Successful snapshots are post-dispatch evidence, not transactional rollback or proof that no shorter transient change occurred.

## Primary references

- [Chrome debugger API](https://developer.chrome.com/docs/extensions/reference/api/debugger)
- [Apple: Take ScreenCaptureKit to the next level](https://developer.apple.com/videos/play/wwdc2022/10155/)
- [Microsoft: create a Windows Graphics Capture item for a window](https://learn.microsoft.com/en-us/windows/win32/api/windows.graphics.capture.interop/nf-windows-graphics-capture-interop-igraphicscaptureiteminterop-createforwindow)
- [Pinned Cua implementation](https://github.com/trycua/cua/tree/0213cd82fd8f5f35d530e7b3eda5286511bbbc10)

See [Computer-use research](COMPUTER_USE_RESEARCH.md) for the complete source comparison and the distinction between capture, input routing, and isolation.
