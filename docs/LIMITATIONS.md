# Limitations

Local Browser Bridge provides exact-window capture and best-effort non-interrupting, target-routed input inside the user's current login session. It does not provide a VM, remote desktop, separate login, separate input seat, or security isolation from that user's applications and credentials.

Setup requirements such as browser loading, matching versions, macOS permissions, and the Windows interactive session are documented separately in [Installation](INSTALL.md). This page describes constraints that remain after correct setup.

## Local token storage

The persisted bearer token is protected against accidental cross-user exposure, not against software already running as the same account, an administrator that takes ownership, or kernel compromise. Inside Chrome, extension storage is restricted to `TRUSTED_CONTEXTS`, which excludes content scripts but does not protect against a compromised extension service worker or popup. The bridge creates or hardens only the exact computed default `.local-browser-bridge` parent under the current user's absolute profile path; missing or non-absolute profile metadata fails closed instead of selecting the working directory, and a matching directory name elsewhere does not establish ownership. It never recursively creates missing ancestors and never rewrites an existing custom parent's permissions. Any custom `LBB_TOKEN_PATH` parent—including the process working directory for a bare relative path—must already be an ordinary, non-link private directory or startup fails before a token is created. Unix requires current-user ownership with exact mode `0700`, creates token files with mode `0600`, opens persisted tokens without following symlinks or blocking on special entries, and rejects multiply linked entries without replacing either name. The complete Unix read, temporary-file, replacement, verification, and cleanup lifecycle remains relative to the same validated directory descriptor, so renaming or substituting the parent path cannot redirect it. A managed Unix directory can have group/other access removed, but missing owner permissions are never added. Windows requires a protected TokenUser-only DACL, rejects a reparse-point final parent, reparse or multiply linked token files, and filesystems that cannot retain that security descriptor. Parent path opening can still traverse an ancestor profile junction; the bridge tolerates that redirection but rechecks the final ordinary parent's stable identity and DACL. All exact-case child opens and creates use the retained parent handle as `NtCreateFile.RootDirectory`; replacement and cleanup use retained file handles. A private typed capability keeps the exact internally created temporary handle open from creation through write, flush, and atomic rename, and every pre-rename failure deletes that handle rather than reopening its old name. Parent-path identity checks detect relocation before a success is reported, but they are not relied on to select a child. This does not defend the secret against another process already running as that same TokenUser.

## Agent Fetch and shell limits

The Agent Fetch protocol makes command invocation possible through a plain GET,
but the private capability and every query value are part of a URL. The bridge
does not log requests and sends no-store/no-referrer headers, yet the calling
agent, browser history, proxy, security product, or screenshot tool can retain
that URL. Use POST with an Authorization header when available, never put
secrets in GET parameters, and rotate the master token if the capability leaks.

Shell support is intentionally disabled by default. Enabling it grants an
authenticated local client all command authority of the signed-in user; shell
commands are not confined to the selected browser tab or native app window.
The server bounds command length, runtime, and retained output, uses null stdin,
and terminates the observed process tree on timeout. A command can still make
persistent system changes, start detached processes before completing, consume
resources within the timeout, or access any data the user can access. There is
no per-command prompt, filesystem allowlist, container, virtual machine, or
privilege reduction.

## Acceptance-tool security boundary

The Windows acceptance coordinator is crash-resistant release tooling, not a
sandbox for hostile candidate code. Its fixed LocalAppData roots, exact ACLs,
staging hashes, predecessor-chain validation, process identities, Job Objects,
and create-once records protect against accidental mix-ups, abandoned shells,
partial launches, and unrelated users. The trusted runner and the frozen,
GitHub-attested candidate still execute as the same signed-in Windows account
that owns those roots. Malicious code already running as that account can alter
or delete the attempt ledger, coordinator records, staged files, and evidence;
the records are not cryptographically authenticated against that account. A
release decision must therefore begin with independently verified GitHub
provenance and treats the candidate as trusted input. Do not use this harness to
analyze an untrusted executable.

Version 0.12.28 ended as a blocked source checkpoint. Version 0.12.29's
stronger named-Job recovery is now Windows-native source-gate proven: one
stable-mutex-protected monotonic deadline covers exact prior termination,
zero-active observation, namespace disappearance, and one final fresh create;
nonzero create status is closed and refused, and publication follows
configuration and binding. This remains non-product SelfTest evidence, not a
shipped or packaged-candidate Windows property. No 0.12.29 candidate was built,
and no acceptance, tag, or Release exists for it. The v0.12.30 candidate passed
both macOS lanes, then failed closed before Windows runner launch because the
staged worker-support loader referenced an undefined hex helper. Version
0.12.31 fixed that loader and passed its quiet macOS lane, but its deliberate
lane failed closed before product dispatch when the bounded post-`ACTION`
fresh-share-frame refresh timed out. Windows candidate execution, stock Chrome,
tagging, and publication did not run for either candidate. Version 0.12.32 then
built one provenance-bound candidate, but its macOS gate failed before
permission probes, quiet-seat stabilization, or candidate process launch when a
source-only handoff self-test printed an extra line. Windows, stock Chrome,
tagging, and publication again did not run. Version 0.12.33 fixed that output
contract and passed both fresh macOS lanes, but its single Windows attempt
failed closed while building the source-bound fixture under nested Jobs. It did
not launch the candidate product, open Chrome, create a tag, or publish. Version
0.12.34 repaired that nested-Job topology; its exact trust gate and both fresh
macOS lanes passed. Its Windows coordinator nevertheless created the persistent
no-retry reservation at `2026-08-26T11:05:46Z` before any coordinator state
existed, and the invoking session was interrupted. A bounded later observation
found no v0.12.34 coordinator directory, evidence directory, candidate process,
listener on port 17373, Computer Use action, or Chrome action. This absence is
not proof that an unobserved transient state never existed. The ledger records a
`not-started` Windows attempt with retry disabled, so v0.12.34 was withdrawn and
was never tagged or published. The bounded observation and its limitations are
preserved in the immutable [v0.12.34 negative-evidence commit](https://github.com/flrngel/local-browser-bridge/tree/aef8fc68018cdb6181ad3d0886acf4e71fcda96d/evidence/v0.12.34/computer/attempts/withdrawn-2509567-windows-pre-coordinator-interruption).

Version 0.12.67 is the current source and schema-3 release-pipeline target. It
retains the breakaway-enabled coordinator Job and atomic private Job-list child
creation, but delays the persistent boundary until private staging and
configuration verification, detached-worker Job binding and guard-ownership
transfer, and runner preparation are complete. The create-once reservation
immediately precedes launch intent and process creation. Ledger presence is a
conservative unknown-outcome boundary: never delete it or retry that product
version. Fresh platform/browser acceptance is still required before
publication; only the immutable
[v0.12.37 GitHub Release](https://github.com/flrngel/local-browser-bridge/releases/tag/v0.12.37)
and its bound evidence receipt can establish that the gate later passed. See
the [Windows acceptance source-gate record](WINDOWS_ACCEPTANCE_HANDOFF.md).

The exact v0.12.63 candidate passed the 207-check macOS quiet lane and its
visual review. Its deliberate lane completed the single exact-app-share action
with a quiet shared input seat and selected a newer same-share frame at
1,784.713 milliseconds estimated age. A new serialized share conversion then
held the helper controller until that frame exceeded the unchanged three-second
lease, and the click failed closed with HTTP 409 `COMPUTER_STALE_FRAME` before
dispatch could be classified. The candidate was not retried and never reached
Windows or publication. Version 0.12.64 reserved a bounded action-admission
interval after publishing an already-aged share frame; it did not extend the
lease or weaken exact target binding.

The exact v0.12.64 candidate then dispatched its refreshed product action and
observed the target postcondition with both shared-seat boundaries quiet. Its
bound completion receipt arrived after 11,765 ms, inside the app and runner's
18-second grace, but an inconsistent 10-second watcher/reader/finalizer bound
rejected it. The candidate was not retried and did not reach Windows or
publication. Version 0.12.66 aligned all completion-receipt boundaries at 18
seconds; it did not weaken receipt identity, chronology, create-once, or
shared-seat requirements. Version 0.12.67 retains that repair.

The exact v0.12.47 candidate passed both packaged macOS lanes and mandatory
visual review, but its coordinator supplied an empty aggregate output path
after combining dependent shell assignments. The finalizer failed closed, so
Windows, stock Chrome, publication, and Release did not run. Version 0.12.48
adds a checked-in wrapper that owns aggregate-directory creation; no v0.12.47
candidate or evidence byte is reusable.

The exact v0.12.48 candidate passed the quiet macOS lane and its visual review.
The deliberate lane observed the separately authorized exact-app button once
without shared-seat activity, then failed closed before product dispatch when
its 1,000 ms evidence-only frame-age ceiling rejected otherwise advancing
same-share frames whose minimum estimated age was 1,040.609619140625 ms.
Version 0.12.49 introduced a 2,500 ms evidence ceiling, preserving a 500 ms
margin inside the product's unchanged three-second stale-frame refusal. Exact
share, target, geometry, advancement, and dispatch-time age checks remain
mandatory; no v0.12.48 candidate or evidence byte is reusable. Its fresh
macOS lanes passed, but Windows stopped before UI use because the readiness
probe's redundant Toolhelp basename filter returned no exact-image child for
the authenticated helper worker. Version 0.12.50 removed only that advisory
filter; direct-parent, exact live full-image-path, PID, protocol-session, and
interactive-session checks remain mandatory. Its fresh quiet macOS lane and
six-image review passed, but a coordinator-supplied nonexistent scratch path
made the deliberate runner stop before candidate execution. The exact v0.12.58
candidate passed both macOS lanes and Windows trust, then failed before UI use
because the authenticated helper stayed stable while Toolhelp returned zero
exact-image direct children for 265 polls. Version 0.12.59 bound the
authenticated controller PID to the exact launched supervisor, the worker PID
to the queried live image path and interactive session, and treated Toolhelp
only as a conflicting-child refusal. Its exact candidate passed both macOS
lanes and Windows trust, but Windows failed before UI use because the
authenticated worker's valid live path spelling did not string-equal the
candidate path. Version 0.12.60 replaces that path-string authority with the
live image file object's volume serial and file index. A same-file alias is
accepted, while a byte-for-byte copy is a distinct object and remains refused.
The exact [v0.12.59 negative record](../evidence/v0.12.59/computer/attempts/withdrawn-ece060c-windows-helper-readiness-image-mismatch/README.md)
contains no paths or candidate bytes. The exact v0.12.60 candidate then passed
both macOS lanes, Windows trust, file-identity helper readiness, and the single
foreground-arm action. Its first `computer.observe` failed closed because the
WGC compositor time appeared slightly ahead after the runner converted the
current QPC value to 100 ns before subtraction. Stock Chrome never started.
The sanitized [v0.12.60 negative record](../evidence/v0.12.60/computer/attempts/withdrawn-7ceb294-windows-wgc-compositor-frame-age/README.md)
contains no paths or candidate bytes. Version 0.12.61 kept the two clocks in a
single rational QPC domain until after subtraction, but its exact candidate
still failed after the one foreground-arm action when the first WGC compositor
timestamp led a later user-mode QPC sample by more than the assumed
quantization tolerance. Stock Chrome never started. Its sanitized negative
record is retained only on the immutable evidence branch
`evidence/v0.12.61-windows-wgc-timestamp-ahead-33271808677`.
Version 0.12.62 preserves positive compositor age exactly and rounds it upward,
but saturates any future lead to zero elapsed age at the callback receipt
boundary. This matches the other native backend's receipt-time upper bound and
avoids treating a newly delivered frame as a clock-domain failure. It requires
entirely fresh acceptance.

The exact v0.12.62 candidate passed both fresh macOS lanes, but its sole Windows
attempt retained only the persistent `reserved-no-retry` record. No matching
coordinator, terminal result, evidence directory, candidate process, listener,
or stock-Chrome record survived, so the protocol classifies the attempt as
`candidate-execution-unknown`. Version 0.12.67 carries the same product fix with
fresh package, candidate, reservation, and acceptance identity; no v0.12.62
artifact or result is reusable.

Coordinator records flush their file contents before an atomic create-once
rename. That survives a dropped remote shell and ordinary process failure, but
does not claim that Windows has committed the parent directory entry across a
sudden machine or storage power loss. Any ambiguous post-crash state is
outcome-unknown and that product version must not be retried.

## Browser limits

- The agent browser must run on the same computer. A cloud-hosted browser cannot reach the user's `127.0.0.1` server.
- Control acts in the selected real browser profile, including its signed-in sessions. A dedicated profile is the clearest way to reduce account exposure.
- Browser-internal pages, extension stores, privileged browser UI, and other debugger-restricted targets are not ordinary controllable web pages.
- Control supports Chrome and Edge 140+. Recursive cross-origin iframe routing is bounded to 16 iframe targets, five levels, and one shared observation budget.
- The debugger lease is exclusive to this extension's controlled tab. Another debugger client, Chrome Cancel, navigation, target loss, or service-worker/connector loss can revoke it.
- The in-page pill and Stop control are defense in depth, not browser chrome. Their public host and private closed-shadow marker are randomized per document; shadow-important resets cover critical host/pseudo/backdrop CSS, and accessibility or View Transition ambiguity fails closed. Content acknowledgement requires the host to remain the direct child of `document.documentElement` before bounded render/layout/computed-style and document/closed-shadow hit tests. The service worker resolves exact `:root`, pins the marker's innermost closed-shadow host, requires that host's immediate parent to equal the root element, requires unique raw `DOM.getTopLayerElements` membership, rejects later same-root ancestry, and checks five `DOM.getNodeForLocation(ignorePointerEventsNone:true)` pill/Stop points outside capture. It repeats host/root ancestry and `hidden`/`inert`/ARIA-critical attributes in the fresh-final phase under a shared 1.5 s/512-step proof budget. Top-layer events use a revision seqlock; only content loss/mismatch increments a separate generation captured before the renderer request, so the bridge's own re-top events do not poison a clean proof and a same-revision loss cannot be absorbed. The content watchdog attempts a new sample every 500 ms only while no earlier acknowledgement is active; its browser round trip is bounded at 2 s, and an uncleared root top-layer event or indicator loss carries an absolute 3 s service-worker deadline plus scheduler/transport timing. Therefore this is not a guaranteed 500 ms revocation bound. Authorized navigation, a browser-native dialog, and intentional capture suspend ordinary input and require proof rebind/restoration rather than extending a dirty timestamp. These browser-process DOM methods are experimental Chrome 140+ dependencies, not compositor or physical-pixel proof. A hostile page can still race after the final proof and before later input, so Chrome's native debugger warning/Cancel and popup release remain independent and authoritative.
- Full Access intentionally bypasses most action-level interlocks. Safe mode is heuristic and cannot determine human intent from every icon, canvas, ambiguous label, or hostile page.
- A DOM ref or screenshot can become stale before an action. The bridge revalidates several identities and proofs, but it does not yet calculate a fresh target-patch SSIM and full-frame visual diff before every command.

## Native capture limits

### Shared constraints

- The helper captures one selected native window, not a complete application workflow. A dialog, popover, tooltip, or child window with a different native window ID can be omitted.
- The selected window must be on screen, non-minimized, and have nonzero size when a live share starts.
- Protected or DRM content can be blank. Secure surfaces and some GPU-rendered applications can stop or refuse capture.
- The live feed uses bounded PNG events with a requested 1–10 FPS maximum cadence and a 1,000,000-pixel image cap, not a video codec, WebRTC stream, or audio stream. Actual delivery can be lower when the compositor is change-driven, semantic enumeration or encoding takes longer, acknowledgement pacing applies backpressure, or an action temporarily owns the serialized helper controller.
- On macOS, `computer.observe` remains a one-shot snapshot while `computer.share.start` uses `SCStream`. On Windows, one-shot observation starts the same bounded WGC implementation as live sharing and stops it after one fresh frame. The different lifetimes and shutdown paths still require separate tests.
- The system cursor is excluded. The visible helper pointer is composited into returned images and is not a native desktop cursor overlay.
- Native capture callbacks continue replacing a one-frame source slot while an action runs, but PNG conversion and protocol publication resume after that serialized action completes. Shared frames show the settled helper pointer; they are not guaranteed to reproduce every intermediate pointer-animation sample.
- An outcome-unknown native mutation—including one whose REST client disconnects after dispatch—keeps a session-scoped publication quarantine even after a fresh one-shot observation or share start. Handler teardown fences the call before releasing the action lock, and no replayable 504 or queued fresh mutation can pass before that quarantine is installed. Until explicit recovery, mutations receive `NO_COMPUTER_FRAME`; this is deliberate, and late frames and capture errors from the old share stay harmless. The server remembers up to 256 retired share epochs and then returns `COMPUTER_SHARE_SESSION_EXHAUSTED` with the `reconnect` recovery hint instead of forgetting an old authority. Establish a new helper transport session in that extreme case: relaunch the macOS helper, or restart the Windows helper if its transport otherwise remains healthy.
- Share lifecycle success is intentionally strict. A raw start result must contain boolean `active: true` and one nonempty bounded `id`; a raw stop result must contain boolean `active: false`. Missing, null, string, sanitized, or contradictory fields are not teardown or startup proof. A rejected start can already have crossed into native capture, so the server quarantines its authority and runs exact-session stop/revocation cleanup independently of the requesting task. If cleanup cannot prove the raw stop result, the originating WebSocket is closed even when its ordinary 64-message queue is full. This can disconnect a helper whose native side effect did not actually occur, which is the deliberate fail-closed outcome.

### macOS

The live-share backend uses `SCStream` and a desktop-independent exact-window filter, but the current selector enumerates on-screen windows and rejects minimized targets. Apple documents that ScreenCaptureKit can continue an exact-window stream while the source is occluded, offscreen, or on another Space, and pauses it while minimized. This project does not advertise those broader behaviors until the packaged helper proves them across supported macOS versions. [Apple's exact-window behavior](https://developer.apple.com/videos/play/wwdc2022/10155/).

There is no `SCContentSharingPicker`. The user chooses a window in the bridge control page, after which the helper starts a programmatic exact `(PID, CGWindowID)` stream. macOS still owns its capture indicator and stop affordance, whose appearance varies by release, but that is not a system picker or an independent desktop.

The helper detects a moved or resized exact window on the next complete native frame. It advances a geometry authority epoch, discards queued old-geometry observations, and updates the existing `SCStream` configuration in place for size or display-scale changes without replacing the share lease. Pre-update callbacks are rejected by both a ScreenCaptureKit host-time boundary and the configured pixel dimensions; if no geometry-bound frame arrives within five seconds, the share fails closed. macOS 14 and later can use the filter's point-to-pixel scale before the transport cap; macOS 13 falls back to the validated enumerated window size and can produce lower-resolution Retina frames. Blank, protected, or other non-complete samples can delay resize detection, while the three-second action-frame lease still expires normally.

The macOS helper has no reconnecting supervisor. Any intentional or unexpected loss of the server WebSocket terminates the helper process without synchronously waiting for `SCStream` teardown; relaunch it after the server is available. This contains a potentially wedged teardown at the process boundary but also means a brief server restart ends that helper run. Explicit `computer.share.stop` is different: it stops capture in process and keeps the helper available.

### Windows

The capture backend is project-owned Windows Graphics Capture using a `CreateFreeThreaded` frame pool on a dedicated MTA owner thread for an exact `(PID, HWND)`. It leaves the normal capture border setting under Windows control and does not request a borderless entitlement. The exact indicator or border depends on Windows version and policy.

WGC owner readiness, first-frame readiness, and startup-only rollback share one
absolute ten-second budget inside the helper's unchanged twelve-second command
watchdog. Shutdown joins the owner only after it has confirmed exit; a driver or
native validation call that remains blocked past the internal deadline leaves
the unconfirmed thread to the disposable worker's termination boundary and
returns a fatal capture-stop result so a fresh helper session is required.

The Windows transport currently requests SDR `B8G8R8A8UIntNormalized` frames and converts BGRA8 to PNG. [Microsoft recommends a full `R16G16B16A16_FLOAT` pipeline when HDR is enabled](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture); until that color-management path is implemented, HDR content can look washed out or clipped. This affects color fidelity, not the exact-window identity boundary.

Minimized windows and applications that stop rendering can freeze or stop producing useful frames. Protected content, secure desktops, elevated targets, and some graphics frameworks remain unsupported or unverified.

## Native input limits

### macOS

- Accessibility actions are preferred, but application accessibility trees can be sparse, stale, or incomplete, especially for Electron, canvas, games, and custom controls.
- Pixel/key delivery depends partly on dynamically resolved private SkyLight symbols. These interfaces are undocumented and unsupported by Apple. A macOS update can rename, gate, or change them; the helper reports the route unavailable rather than falling back to global HID. Source and per-architecture packaged Mach-O audits forbid known global cursor/HID APIs and freeze the expected targeted symbol set, but an audit cannot make the private route supported or prove that an application consumed an event.
- Target-routed input requires Accessibility permission, including pointer move, click, drag, scroll, and native text. A different-process focus lease reads the user-front app's and target app's exact `AXFocusedWindow` and `AXMainWindow` values and requires the later user sample to equal the original snapshot. Multi-window routing is admitted only when each app's initial main/focused pair agrees, both retained target windows expose writable `AXMain`, and independent WindowServer owner-connection lookups bind both target IDs to one exact PSN. The requested sibling is selected and read back through exact-window `AXMain` while the user is still fully active; application-level `AXFocusedWindow` is never written. The private route then leaves WindowServer's user-front PSN/PID unchanged while the saved user app's `AXFrontmost` changes from true to false and the exact target becomes `AXFrontmost=true` only while its Focus record is active. When the target app had a distinct prior receiver, a bounded target-only make-key pair commits the requested AppKit receiver after activation and the requested-active phase is re-proved before dispatch. Normal preparation records end authorization with a front-PSN → exact saved-user AX window/phase-appropriate `AXFrontmost` → front-PSN sandwich before and after dispatch accounting. Cleanup defocuses requested, restores a distinct prior receiver through an exact target Focus plus a target-only make-key pair when AppKit needs it, proves prior main+focused, defocuses prior, and restores the user; it never calls `_SLPSSetFrontProcessWithOptions`, raises a window, or changes Space. Cleanup failure dominates the original action result. Fixed deadline slices reserve time for one safe user-only compensation. That emergency Focus is deliberately target-independent: it requires stable saved front PSN/PID, exact saved window with `AXFrontmost=false`, raw user restorability, and the original deadline; target uncertainty still returns an unknown outcome. Before each keyboard down and each new target-routed pointer mutation, including every point in a pointer trajectory, the exact released owner and requested-active target receiver are re-proved immediately before bounded dispatch; long drag rechecks before each drag event while release remains unconditional after mouse-down. The unrelated foreground app is queried read-only; only the selected target may receive the existing one-time Chromium Accessibility opt-in. Missing, changed, or unreadable identity fails closed. Pointer trajectories use the same bounded lease and exact prior-receiver restoration instead of attempting delivery to an inactive same-process window.
- Current mutable targets are limited to non-minimized windows on the active Space. ScreenCaptureKit's ability to capture another Space does not grant cross-Space input.
- A process can have multiple real windows, and on the tested macOS build ScreenCaptureKit adds a same-PID, layer-0 `AXDialog` for its title-bar indicator. Neither raw WindowServer sibling count nor AX-window count proves the keyboard receiver. Delivery instead requires the application's exact [`AXFocusedWindow`](https://developer.apple.com/documentation/applicationservices/kaxfocusedwindowattribute) to match the requested CGWindowID immediately before key-down; text also requires its focused element to resolve to that exact window. If the user's foreground process owns a different sibling, focus-capable input is refused before dispatch. For a different-process target, the stricter main/focused/settable/owner admission above supports a distinct sibling and requires exact restoration; apps that expose a genuine main/key split, reject `AXMain`, fail the target-only make-key transfer, or cannot be proved within the deadline remain unsupported and fail closed.
- Native `computer.typeText` is limited to 2,000 UTF-16 code units, paced between each Unicode scalar, and re-proves the exact focused window and focused-element owner immediately before every scalar key-down. Proof loss after an earlier scalar is `COMPUTER_OUTCOME_UNKNOWN` and must not be automatically retried; key-up remains unconditional after a posted key-down. Apple notes that [application frameworks may ignore the Unicode string attached to a Quartz keyboard event](https://developer.apple.com/documentation/coregraphics/cgevent/keyboardsetunicodestring%28stringlength%3Aunicodestring%3A%29), so successful event posting remains delivery evidence rather than a confirmed text postcondition. Prefer semantic `setValue` for a field that exposes it.
- Cross-Space keyboard and pointer input is not claimed. It would require additional private Space discovery/routing, OS-version gating, and invariant tests before release.
- Exact receiver checks and restore polling narrow but cannot eliminate TOCTOU: there is no public atomic primitive that binds an AX focus proof to the following private PID event post. Short per-element timeouts, bounded ancestry, and deadline checks fail closed within the native text budget, but one unresponsive provider call can still consume its timeout interval. `CGWindowListCopyWindowInfo` is synchronous and cannot be interrupted in flight; every inventory is checked against the same absolute proof deadline before and after it returns, and a late result authorizes no focus record.
- `SLEventPostToPid` returns no acceptance value. A recorded call is a dispatch attempt, not an operating-system delivery receipt and not target-effect proof. Accessibility return values similarly stop at the API boundary; semantic `setValue` becomes `Confirmed` only after the target value or permitted masked length is read back.
- Secure input, protected controls, some Chromium gestures, right-click variants, games, and HID-only engines can reject background events.
- Native `computer.key` maps only the documented navigation/editing keys, F1–F12, ASCII letters/digits, selected US-keyboard punctuation, and Control/Alt/Shift/Meta modifiers. Other names accepted by browser `page.key` fail closed; use `computer.typeText` for text.

### Windows

- `inputReady` and `semanticReady` are conservative environment probes, not universal provider guarantees. They are false in Session 0 or when the helper cannot read the input desktop, foreground HWND, GUI-thread focus HWND, or hardware cursor; even a true probe can be followed by an application-specific UIA or exact-HWND refusal.
- UI Automation works only when the target exposes a useful pattern and accepts it without disruptive focus changes. A snapshot visits at most 1,500 Control View nodes, 25 levels, and 500 actionable controls, with a 750 ms traversal budget checked between provider calls. Elements collected before a limit remain usable, while `semanticTruncated` and its closed-vocabulary reason disclose that later controls can be absent.
- Individual UI Automation provider calls cannot be cancelled safely. Windows therefore performs controller work in a disposable supervised worker with a 12-second hard operation deadline. If a provider stalls, the worker is terminated and restarted; an action that crossed its side-effect boundary is reported as outcome-unknown and is never retried automatically.
- Server-transport loss also terminates the disposable Windows worker. The still-running supervisor starts replacements with backoff and reconnects a worker when the server returns. Explicit `computer.share.stop` stays inside the current worker and does not trigger replacement.
- API cancellation and REST-client teardown are cooperative and remove server-side authority before publishing an outcome-unknown replay, but neither is rollback. A native call already inside an operating-system provider can finish cleanup after the HTTP 202 or client disconnect; Windows retains the 12-second disposable-worker containment boundary. If the async runtime is already shutting down, owner-bound interrupted replay entries deliberately remain in-flight rather than exposing a 504 before quarantine; the terminating server discards that registry with its state.
- A browser side effect can reach Chrome before cancellation, caller disconnect, or connector timeout becomes visible. These controlled-page outcome-unknown paths preserve the debugger lease when safe but quarantine observation-derived mutations until explicit `page.observe`; failed extension turn persistence revokes the lease. Browser-process tab mutations do not use that page-authority quarantine: a canceled `tabs.activate`, `tabs.new`, or `tabs.close` is still outcome-unknown and must be reconciled with `tabs.list`, never retried under a new `callId`. A late `tabs.new` blocks the global browser-action queue—including trusted popup approval dispatch—until every created tab's bridge provenance is durable and canceled-command freshness finalization is complete; this also keeps an omitted-URL blank tab visible to Safe-mode reconciliation. The canceled caller can receive its outcome before that internal queue barrier releases.
- Exact-HWND messages are application-framework behavior, not a universal trusted input API. A successful [`PostMessage`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew) call proves only that the message was queued.
- The shared-pointer monitor owns one message-only [`RIDEV_INPUTSINK`](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input) mouse registration in the disposable helper worker and a minimal dedicated-thread [`WH_MOUSE_LL`](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelmouseproc) epoch for generic/injected flags. It stores counters and health only, never Raw Input payloads, coordinates, or device identity. Microsoft documents that a low-level hook that exceeds its timeout can be silently removed with no notification; `pointerActivityMonitorHealthy` therefore means initialization and the sampled epoch were readable, not that hook delivery was continuously provable. Raw Input, injected flags, integrity levels, virtual devices, remote transport, and eventless or out-and-back cursor changes still have blind spots. An unexplained sampled delta becomes `unknown` and fails closed, but two equal coordinates do not prove that nothing happened between them.
- Native `computer.typeText` accepts only Unicode window recipients, uses a documented `WM_CHAR` repeat count of one, and limits each command to 2,000 UTF-16 code units. It posts at most 16 code units before a scheduler pause and checks cancellation at every unit. This caps one command at half of Windows' documented 4,000 minimum posted-message queue limit, but it still cannot prove that a control consumed the queued text; semantic `setValue` is the confirmed route when available.
- Chromium, Electron, WPF, WinUI, GTK, games, canvas, elevated processes, and UIPI boundaries can reject background delivery. Browser web content should use the extension instead.
- The helper does not use global [`SendInput`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) as an automatic fallback. Unsupported actions fail closed.
- Native `computer.key` supports the documented navigation/editing keys, F1–F12, ASCII letters/digits, selected punctuation, and Control/Alt/Shift. Windows-key chords and global or secure combinations such as `Alt+Tab`, `Control+Escape`, and `Control+Alt+Delete` are refused.

## Non-interruption is not isolation

On a supported application, a sealed target route can leave the user's foreground application, platform focus oracle, and active desktop unchanged without asking the helper to move the global pointer. That is not the same as requiring two shared cursor samples to be equal. `cursorPositionUnchanged` is diagnostic: the person, a virtual-HID source, a remote session, or another process can move the pointer concurrently. On macOS a healthy HID-system counter boundary can corroborate that such activity occurred, but it does not identify a physical device or rule out every eventless warp. The action therefore reports `helperGlobalPointerPreservation`, `sharedPointerBoundaryCorroborated`/`sharedPointerBoundaryState`, `hidSystemPointerActivityObserved`, `pointerActivityMonitorHealthy`, and `sharedPointerActivityState` separately. `contaminated` means shared activity was observed; `unknown` fails closed.

The helper's exact route, an operating-system API return, and the target application's postcondition are also different facts. A sealed `inputDelivery` proves the attempted route and absence of helper-requested global input. API success proves at most acceptance or queueing where the API has such a signal. Only an application-owned read-back can confirm the requested effect.

On macOS the general before/after snapshot derives the front window from a read-only AX focused-window sample—not the first same-PID compositor row—and sandwiches AX, pointer activity, and Space sampling between the same front ProcessSerialNumber/PID. Every focus-preparing route additionally captures, stabilizes, restores, and re-proves that exact Accessibility window. Windows also includes the GUI-thread focus window. These are bounded observations, not an atomic rollback guarantee or proof that no unobserved transient focus change occurred. Both the user and helper still share:

- the same login session and security principal;
- application files, settings, network access, and signed-in accounts;
- operating-system permissions and much of the same clipboard/application state; and
- one underlying WindowServer or Windows desktop environment.

Same-session operation is cooperative: it neither blocks the person from acting nor gives the helper an independent focus, cursor, or input queue. True independent concurrency, hostile workloads, or destructive workloads require an explicitly managed VM, RDP/other desktop session, separate login, or other sandbox. PiP automation, virtual displays, VM/RDP lifecycle, and separate OS input seats are not included.

## Product comparison boundary

[OpenAI's current documentation](https://learn.chatgpt.com/docs/computer-use) says macOS Computer Use can run a scoped task in the background and says Windows Computer Use runs on the active desktop. OpenAI publishes the macOS Screen Recording and Accessibility prerequisites, but it does not publish its native capture or input implementation. Any claim that Codex specifically uses ScreenCaptureKit, SkyLight, or a particular private symbol is inference, not an official implementation detail.

The closest pinned shared-session comparison is [Cua commit `0213cd8`](https://github.com/trycua/cua/tree/0213cd82fd8f5f35d530e7b3eda5286511bbbc10). Its code and write-ups inform compatibility tests, but this project does not claim Cua feature parity or copy its protocol. Microsoft's [Windows child-session documentation](https://learn.microsoft.com/en-us/windows/win32/termserv/child-sessions) defines the separate-session boundary; exact-window WGC alone does not create that seat.

## Evidence gaps

The following need versioned packaged evidence before they can become supported claims:

- persistent SCStream behavior while fully occluded, moved offscreen, or moved to another Space;
- cross-Space macOS input with unchanged frontmost process/window, hardware cursor, and active Space;
- long-running stream recovery after display sleep, permission changes, resize, target closure, macOS helper relaunch, or supervised Windows worker replacement;
- representative Windows WGC and UIA/background-input runs on real Windows hardware;
- minimized, protected, elevated, secure-desktop, multi-display, mixed-DPI, and child-window behavior;
- private macOS input compatibility across every supported macOS release and architecture; and
- concurrent native agents with deterministic per-window leases and conflict handling.

The exact v0.12.8 macOS release candidate passed its 187-check persistent-share
matrix and produced six reviewed screenshots, but that archive was never
published. Its Windows run delivered a fresh foreground-arm request and then
received no click; no received marker was created, and the runner timed out at
`wait-foreground-arm` before the invariant baseline or any observation,
capture, share, input, or other product action. Chrome acceptance was not
started, the protected publication job was canceled, and no v0.12.8 Release
exists. Those records are useful historical evidence, but they did not satisfy
the later v0.12.9 Windows, Chrome, or immutable-release gates.

Version 0.12.9 kept the Windows runner's bounded five-minute arm interval and
native one-click authority unchanged, and added a read-only handoff watcher.
Its one exact packaged macOS run reached the first semantic `setValue` after
40 passing precondition assertions, then failed closed because the sampled
global cursor position changed across `semanticSetValue`. The retained record
cannot identify whether the helper, a person, or another source moved it. The
run stopped before semantic read-back and retained one screenshot. Windows and
stock-Chrome acceptance were not started, the protected publication job was
canceled, and no v0.12.9 Release exists. See the
[withdrawn attempt](../evidence/v0.12.9/computer/attempts/withdrawn-db624da-macos-semantic-hardware-cursor-change/README.md).

The exact v0.12.10 packaged macOS attempt exercised the replacement attribution
model through 69 passing assertions and six fixture-only screenshots. It then
waited 300 seconds for separately authorized pointer movement, observed none,
and failed before dispatching the post-resize product action. That is a valid
negative handoff result, not product-action success. The candidate was not
retried; Windows and stock-Chrome were not started, publication was canceled,
and no v0.12.10 Release exists.

Version 0.12.11 passed its build and provenance gates but was withdrawn before
execution. Its release receipt had one macOS result digest even though policy
required fresh, non-mergeable quiet and deliberate-concurrency lanes; one hash
could not authenticate both without collapsing the boundary the test was meant
to prove.

Version 0.12.12 was withdrawn after three deliberate-concurrency attempts all
stopped with no product dispatch. One final SystemProbe inherited only the
nearly exhausted arm-deadline budget; one arm interval contained disallowed
pointer input; and one `ACTION` transition contained input plus foreground/focus
contamination. Those failures remain valid negative evidence, not proof of a
helper action.

Version 0.12.13 was withdrawn after its quiet macOS lane completed 192 of 193
assertions. All product/fixture cells passed, but the unchanged final whole-run
boundary observed unrelated shared-seat `mouseMoved`/cursor activity. That is
valid contamination evidence, not helper attribution. The deliberate lane,
Windows, and stock-Chrome were not started and publication was canceled.

Version 0.12.20 passed its quiet macOS lane but timed out before product dispatch
in its then-mandatory physical-pointer lane. Its Windows run separately timed out
because the action surface could not be found under a state-mutating title.
Both outcomes are preserved as negative evidence; Chrome never started and no
Release was published. That pointer lane has been historical since v0.12.24; its tools
remain optional adversarial coverage and cannot satisfy publication.

The exact v0.12.22 quiet run later returned a Confirmed semantic action with a
healthy, corroborated, quiet sealed pointer record, but its harness required
keyboard fields that exist only on independent SystemProbe samples. It failed
closed after 55 of 56 checks. That was a harness false negative, not proof of an
unsafe action; deliberate macOS, Windows, and Chrome did not run, publication
was canceled, and no v0.12.22 Release exists.

Version 0.12.23 separated that sealed action-pointer classifier from the
keyboard-aware independent-system classifier without weakening either. Its
quiet packaged lane passed 208/208 checks. Its deliberate lane accepted the
exact app-share start receipt and completed 89/89 recorded assertions, then
reused a pre-handoff stream frame after 43.807 seconds. `computer.click`
correctly returned HTTP 409 `COMPUTER_STALE_FRAME` before dispatch; no completion
receipt, Windows, Chrome, publication, or Release followed. The exact negative
record is retained on the immutable [v0.12.23 evidence commit](https://github.com/flrngel/local-browser-bridge/tree/4e4db75a4ede915d982d139a82dacac8a6c4772a/evidence/v0.12.23/computer/attempts/withdrawn-9e50811-macos-app-share-stale-frame).

Version 0.12.24 added the bounded post-`ACTION` frame refresh, then its exact
Windows read-only handoff watcher failed before operator action because a
closure-created PowerShell 5.1 dynamic module could not resolve the atomic
marker reader. Chrome and publication did not follow.

Version 0.12.25 retained the authority refresh and made that watcher portable
to exact system PowerShell 5.1 by passing marker paths as explicit callback
arguments.

Version 0.12.37 retains sealed route provenance, the v0.12.23 classifier
separation, and conservative shared-pointer monitoring. Its bounded, abortable
post-`ACTION` authority refresh must obtain a strictly newer frame from the same
share, target, and geometry within the reserved deadline before deriving click
authority. Before either lane invokes a candidate executable, fixture, server,
or helper, a native SystemProbe gate requires a 30-second sampled quiet epoch
with at least 60 stable transitions sampled every 500 ms. Pointer, cursor,
foreground, AX-focus/front-window, or active-Space activity resets the complete
epoch under one immutable 30-minute deadline; unknown or unhealthy monitoring
fails immediately. This reduces ambient pre-run contamination but cannot
reserve the shared login seat, so every later per-action and whole-run boundary
remains unchanged and mandatory.

The deliberate lane's separate nonactivating macOS acceptance app is test
orchestration, not a product capability. Its exact bundle/window/button and
create-once request, start, and completion receipts record the acceptance-button
action and an ordered state sequence around the product boundaries. The chain
is orchestration evidence, not notification-only and not product authority. It
cannot prove who or which provider produced the click, authenticate a Computer
Use implementation cryptographically, or turn the shared login session into a
separate input seat.

The narrowed result fields intentionally avoid stronger claims:
`acceptanceButtonActionObserved` reports the app-owned button receipt;
`appShareSurfaceObservedAtProductBoundaries` and
`sampledSharedContextUnchanged` report boundary samples, not continuous custody;
and `sharedHidInputObserved` is `null` in quiet because no app-share transaction
exists but `false` in deliberate when no HID pointer/keyboard activity was
observed across that transaction. The completion marker's
`handoffStateSequenceBound` binds the ordered marker state, not uninterrupted
window observation. Endpoint samples plus cumulative HID pointer/keyboard
counters cannot prove zero transient programmatic changes, a continuous monitor,
atomic provider identity, or zero transient focus/window manipulation. Unknown
state, a duplicate or stale click, changed marker bytes, lost sampled app/window
binding, or observed shared-seat activity fails closed.

Marker publication and receipt reads use create-once names, stable descriptor/path
identity checks, nonblocking and no-follow opens, and deadline checks immediately
before and after every filesystem step. User-space code cannot preempt an
operating-system filesystem call already in progress, so a stalled local
filesystem can delay fail-closed termination. It cannot turn a late marker into
passing evidence.

These source contracts still do not satisfy a release gate: the exact packaged
macOS, interactive-Windows, stock-Chrome, evidence-commit, and immutable-
publication paths must all run fresh. Quiet and deliberate-concurrency evidence
are separate lanes; neither may be relabelled as the other.

The checked-in [evidence index](../evidence/) records what was actually run. A code path, unit test, or transport acknowledgement alone is not evidence that an application accepted the action or that the user's desktop remained unchanged.

## Research references

- [Temporal UI State Inconsistency / PUSV](https://arxiv.org/abs/2604.18860) — observation-to-action race defenses
- [Apple `CGEventSource` counters](https://developer.apple.com/documentation/coregraphics/cgeventsource/counterforeventtype(_:eventtype:)) — system-source activity counters, not physical-device identity
- [ParaGUIBench](https://arxiv.org/abs/2607.22689) — parallel GUI evaluation through separate desktop instances
- [UFO²](https://arxiv.org/abs/2504.14603) — isolated virtual desktop as a distinct architecture
- [CaMeLs Can Use Computers Too](https://arxiv.org/abs/2601.09923) — security isolation for computer-use agents
- [Computer-use research](COMPUTER_USE_RESEARCH.md) — pinned implementation and community review
