# Limitations

Local Browser Bridge provides exact-window capture and best-effort non-interrupting, target-routed input inside the user's current login session. It does not provide a VM, remote desktop, separate login, separate input seat, or security isolation from that user's applications and credentials.

Setup requirements such as browser loading, matching versions, macOS permissions, and the Windows interactive session are documented separately in [Installation](INSTALL.md). This page describes constraints that remain after correct setup.

## Local token storage

The persisted bearer token is protected against accidental cross-user exposure, not against software already running as the same account, an administrator that takes ownership, or kernel compromise. Inside Chrome, extension storage is restricted to `TRUSTED_CONTEXTS`, which excludes content scripts but does not protect against a compromised extension service worker or popup. The bridge creates or hardens only the exact computed default `.local-browser-bridge` parent under the current user's absolute profile path; missing or non-absolute profile metadata fails closed instead of selecting the working directory, and a matching directory name elsewhere does not establish ownership. It never recursively creates missing ancestors and never rewrites an existing custom parent's permissions. Any custom `LBB_TOKEN_PATH` parent—including the process working directory for a bare relative path—must already be an ordinary, non-link private directory or startup fails before a token is created. Unix requires current-user ownership with exact mode `0700`, creates token files with mode `0600`, opens persisted tokens without following symlinks or blocking on special entries, and rejects multiply linked entries without replacing either name. The complete Unix read, temporary-file, replacement, verification, and cleanup lifecycle remains relative to the same validated directory descriptor, so renaming or substituting the parent path cannot redirect it. A managed Unix directory can have group/other access removed, but missing owner permissions are never added. Windows requires a protected current-user-only DACL, rejects a reparse-point parent or token and a multiply linked token file, and fails closed on filesystems that cannot retain that security descriptor. Its validated directory handle and pathname-ancestor leases deny delete sharing for the whole transaction, and stable file identity is rechecked around path-based child operations. Those ancestor handles are transient and never change ancestor ACLs or other metadata, which matters for redirected profiles and managed storage.

## Browser limits

- The agent browser must run on the same computer. A cloud-hosted browser cannot reach the user's `127.0.0.1` server.
- Control acts in the selected real browser profile, including its signed-in sessions. A dedicated profile is the clearest way to reduce account exposure.
- Browser-internal pages, extension stores, privileged browser UI, and other debugger-restricted targets are not ordinary controllable web pages.
- Control supports Chrome and Edge 140+. Recursive cross-origin iframe routing is bounded to 16 iframe targets, five levels, and one shared observation budget.
- The debugger lease is exclusive to this extension's controlled tab. Another debugger client, Chrome Cancel, navigation, target loss, or service-worker/connector loss can revoke it.
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
- An outcome-unknown native mutation—including one whose REST client disconnects after dispatch—keeps a session-scoped publication quarantine even after a fresh one-shot observation or share start. Handler teardown fences the call before releasing the action lock, and no replayable 504 or queued fresh mutation can pass before that quarantine is installed. Until explicit recovery, mutations receive `NO_COMPUTER_FRAME`; this is deliberate, and late frames and capture errors from the old share stay harmless. The server remembers up to 256 retired share epochs and then returns `COMPUTER_SHARE_SESSION_EXHAUSTED` with the `reconnect` recovery hint instead of forgetting an old authority; reconnect the helper to obtain a new session in that extreme case.

### macOS

The live-share backend uses `SCStream` and a desktop-independent exact-window filter, but the current selector enumerates on-screen windows and rejects minimized targets. Apple documents that ScreenCaptureKit can continue an exact-window stream while the source is occluded, offscreen, or on another Space, and pauses it while minimized. This project does not advertise those broader behaviors until the packaged helper proves them across supported macOS versions. [Apple's exact-window behavior](https://developer.apple.com/videos/play/wwdc2022/10155/).

There is no `SCContentSharingPicker`. The user chooses a window in the bridge control page, after which the helper starts a programmatic exact `(PID, CGWindowID)` stream. macOS still owns its capture indicator and stop affordance, whose appearance varies by release, but that is not a system picker or an independent desktop.

The helper detects a moved or resized exact window on the next complete native frame. It advances a geometry authority epoch, discards queued old-geometry observations, and updates the existing `SCStream` configuration in place for size or display-scale changes without replacing the share lease. Pre-update callbacks are rejected by both a ScreenCaptureKit host-time boundary and the configured pixel dimensions; if no geometry-bound frame arrives within five seconds, the share fails closed. macOS 14 and later can use the filter's point-to-pixel scale before the transport cap; macOS 13 falls back to the validated enumerated window size and can produce lower-resolution Retina frames. Blank, protected, or other non-complete samples can delay resize detection, while the three-second action-frame lease still expires normally.

### Windows

The capture backend is project-owned Windows Graphics Capture using a `CreateFreeThreaded` frame pool on a dedicated MTA owner thread for an exact `(PID, HWND)`. It leaves the normal capture border setting under Windows control and does not request a borderless entitlement. The exact indicator or border depends on Windows version and policy.

The Windows transport currently requests SDR `B8G8R8A8UIntNormalized` frames and converts BGRA8 to PNG. [Microsoft recommends a full `R16G16B16A16_FLOAT` pipeline when HDR is enabled](https://learn.microsoft.com/en-us/windows/uwp/audio-video-camera/screen-capture); until that color-management path is implemented, HDR content can look washed out or clipped. This affects color fidelity, not the exact-window identity boundary.

Minimized windows and applications that stop rendering can freeze or stop producing useful frames. Protected content, secure desktops, elevated targets, and some graphics frameworks remain unsupported or unverified.

## Native input limits

### macOS

- Accessibility actions are preferred, but application accessibility trees can be sparse, stale, or incomplete, especially for Electron, canvas, games, and custom controls.
- Pixel/key delivery depends partly on dynamically resolved private SkyLight symbols. A macOS update can rename, gate, or change them. The helper reports the route unavailable rather than falling back to global HID.
- Focus-capable target-routed input requires Accessibility permission, including pixel click, drag, and scroll. A different-process focus lease reads the user-front app's and target app's exact `AXFocusedWindow` and `AXMainWindow` values and requires the later user sample to equal the original snapshot. Multi-window routing is admitted only when each app's initial main/focused pair agrees, both retained target windows expose writable `AXMain`, and independent WindowServer owner-connection lookups bind both target IDs to one exact PSN. The requested sibling is selected and read back through exact-window `AXMain` while the user is still fully active; application-level `AXFocusedWindow` is never written. The private route then leaves WindowServer's user-front PSN/PID unchanged while the saved user app's `AXFrontmost` changes from true to false and the exact target becomes `AXFrontmost=true` only while its Focus record is active. Normal preparation records end authorization with a front-PSN → exact saved-user AX window/phase-appropriate `AXFrontmost` → front-PSN sandwich before and after dispatch accounting. Cleanup defocuses requested, restores a distinct prior receiver through an exact target Focus plus a target-only make-key pair when AppKit needs it, proves prior main+focused, defocuses prior, and restores the user; it never calls `_SLPSSetFrontProcessWithOptions`, raises a window, or changes Space. Cleanup failure dominates the original action result. Fixed deadline slices reserve time for one safe user-only compensation. That emergency Focus is deliberately target-independent: it requires stable saved front PSN/PID, exact saved window with `AXFrontmost=false`, raw user restorability, and the original deadline; target uncertainty still returns an unknown outcome. Before each keyboard down and each new focus-capable pointer mutation, the exact released owner and requested-active target receiver are re-proved; long drag rechecks before each drag event while release remains unconditional after mouse-down. The unrelated foreground app is queried read-only; only the selected target may receive the existing one-time Chromium Accessibility opt-in. Missing, changed, or unreadable identity fails closed. Pointer trajectory alone does not prepare focus, but `inputReady` reports the requirements for the complete input backend.
- Current mutable targets are limited to non-minimized windows on the active Space. ScreenCaptureKit's ability to capture another Space does not grant cross-Space input.
- A process can have multiple real windows, and on the tested macOS build ScreenCaptureKit adds a same-PID, layer-0 `AXDialog` for its title-bar indicator. Neither raw WindowServer sibling count nor AX-window count proves the keyboard receiver. Delivery instead requires the application's exact [`AXFocusedWindow`](https://developer.apple.com/documentation/applicationservices/kaxfocusedwindowattribute) to match the requested CGWindowID immediately before key-down; text also requires its focused element to resolve to that exact window. If the user's foreground process owns a different sibling, focus-capable input is refused before dispatch. For a different-process target, the stricter main/focused/settable/owner admission above supports a distinct sibling and requires exact restoration; apps that expose a genuine main/key split, reject `AXMain`, fail the target-only make-key transfer, or cannot be proved within the deadline remain unsupported and fail closed.
- Native `computer.typeText` is limited to 2,000 UTF-16 code units, paced between each Unicode scalar, and re-proves the exact focused window and focused-element owner immediately before every scalar key-down. Proof loss after an earlier scalar is `COMPUTER_OUTCOME_UNKNOWN` and must not be automatically retried; key-up remains unconditional after a posted key-down. Apple notes that [application frameworks may ignore the Unicode string attached to a Quartz keyboard event](https://developer.apple.com/documentation/coregraphics/cgevent/keyboardsetunicodestring%28stringlength%3Aunicodestring%3A%29), so successful event posting remains delivery evidence rather than a confirmed text postcondition. Prefer semantic `setValue` for a field that exposes it.
- Cross-Space keyboard and pointer input is not claimed. It would require additional private Space discovery/routing, OS-version gating, and invariant tests before release.
- Exact receiver checks and restore polling narrow but cannot eliminate TOCTOU: there is no public atomic primitive that binds an AX focus proof to the following private PID event post. Short per-element timeouts, bounded ancestry, and deadline checks fail closed within the native text budget, but one unresponsive provider call can still consume its timeout interval. `CGWindowListCopyWindowInfo` is synchronous and cannot be interrupted in flight; every inventory is checked against the same absolute proof deadline before and after it returns, and a late result authorizes no focus record.
- Secure input, protected controls, some Chromium gestures, right-click variants, games, and HID-only engines can reject background events.

### Windows

- UI Automation works only when the target exposes a useful pattern and accepts it without disruptive focus changes. A snapshot visits at most 1,500 Control View nodes, 25 levels, and 500 actionable controls, with a 750 ms traversal budget checked between provider calls. Elements collected before a limit remain usable, while `semanticTruncated` and its closed-vocabulary reason disclose that later controls can be absent.
- Individual UI Automation provider calls cannot be cancelled safely. Windows therefore performs controller work in a disposable supervised worker with a 12-second hard operation deadline. If a provider stalls, the worker is terminated and restarted; an action that crossed its side-effect boundary is reported as outcome-unknown and is never retried automatically.
- API cancellation and REST-client teardown are cooperative and remove server-side authority before publishing an outcome-unknown replay, but neither is rollback. A native call already inside an operating-system provider can finish cleanup after the HTTP 202 or client disconnect; Windows retains the 12-second disposable-worker containment boundary. If the async runtime is already shutting down, owner-bound interrupted replay entries deliberately remain in-flight rather than exposing a 504 before quarantine; the terminating server discards that registry with its state.
- A browser side effect can reach Chrome before cancellation, caller disconnect, or connector timeout becomes visible. These controlled-page outcome-unknown paths preserve the debugger lease when safe but quarantine observation-derived mutations until explicit `page.observe`; failed extension turn persistence revokes the lease. Browser-process tab mutations do not use that page-authority quarantine: a canceled `tabs.activate`, `tabs.new`, or `tabs.close` is still outcome-unknown and must be reconciled with `tabs.list`, never retried under a new `callId`. A late `tabs.new` does block the extension queue until its bridge provenance is durable, so Safe mode can list the created blank tab.
- Exact-HWND messages are application-framework behavior, not a universal trusted input API. A successful [`PostMessage`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-postmessagew) call proves only that the message was queued.
- Native `computer.typeText` accepts only Unicode window recipients, uses a documented `WM_CHAR` repeat count of one, and limits each command to 2,000 UTF-16 code units. It posts at most 16 code units before a scheduler pause and checks cancellation at every unit. This caps one command at half of Windows' documented 4,000 minimum posted-message queue limit, but it still cannot prove that a control consumed the queued text; semantic `setValue` is the confirmed route when available.
- Chromium, Electron, WPF, WinUI, GTK, games, canvas, elevated processes, and UIPI boundaries can reject background delivery. Browser web content should use the extension instead.
- The helper does not use global [`SendInput`](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput) as an automatic fallback. Unsupported actions fail closed.

## Non-interruption is not isolation

On a supported application, target-routed input can leave the user's foreground application, platform focus oracle, hardware cursor, and active desktop unchanged. On macOS the general before/after snapshot derives the front window from a read-only AX focused-window sample—not the first same-PID compositor row—and sandwiches AX, cursor, and Space sampling between the same front ProcessSerialNumber/PID. Every focus-preparing route additionally captures, stabilizes, restores, and re-proves that exact Accessibility window. Windows also includes the GUI-thread focus window. These are bounded observations, not an atomic rollback guarantee or proof that no unobserved transient focus change occurred. Both the user and helper still share:

- the same login session and security principal;
- application files, settings, network access, and signed-in accounts;
- operating-system permissions and much of the same clipboard/application state; and
- one underlying WindowServer or Windows desktop environment.

Hostile or destructive workloads require an explicitly managed VM, RDP desktop, separate login, or other sandbox. PiP automation, virtual displays, VM/RDP lifecycle, and separate OS input seats are not included.

## Product comparison boundary

[OpenAI's current documentation](https://learn.chatgpt.com/docs/computer-use) says macOS Computer Use can run a scoped task in the background and says Windows Computer Use runs on the active desktop. OpenAI publishes the macOS Screen Recording and Accessibility prerequisites, but it does not publish its native capture or input implementation. Any claim that Codex specifically uses ScreenCaptureKit, SkyLight, or a particular private symbol is inference, not an official implementation detail.

The closest pinned shared-session comparison is [Cua commit `0213cd8`](https://github.com/trycua/cua/tree/0213cd82fd8f5f35d530e7b3eda5286511bbbc10). Its code and write-ups inform compatibility tests, but this project does not claim Cua feature parity or copy its protocol. Microsoft's [Windows child-session documentation](https://learn.microsoft.com/en-us/windows/win32/termserv/child-sessions) defines the separate-session boundary; exact-window WGC alone does not create that seat.

## Evidence gaps

The following need versioned packaged evidence before they can become supported claims:

- persistent SCStream behavior while fully occluded, moved offscreen, or moved to another Space;
- cross-Space macOS input with unchanged frontmost process/window, hardware cursor, and active Space;
- long-running stream recovery after display sleep, permission changes, resize, target closure, and helper reconnect;
- representative Windows WGC and UIA/background-input runs on real Windows hardware;
- minimized, protected, elevated, secure-desktop, multi-display, mixed-DPI, and child-window behavior;
- private macOS input compatibility across every supported macOS release and architecture; and
- concurrent native agents with deterministic per-window leases and conflict handling.

The checked-in [evidence index](../evidence/) records what was actually run. A code path, unit test, or transport acknowledgement alone is not evidence that an application accepted the action or that the user's desktop remained unchanged.

## Research references

- [Temporal UI State Inconsistency / PUSV](https://arxiv.org/abs/2604.18860) — observation-to-action race defenses
- [UFO²](https://arxiv.org/abs/2504.14603) — isolated virtual desktop as a distinct architecture
- [CaMeLs Can Use Computers Too](https://arxiv.org/abs/2601.09923) — security isolation for computer-use agents
- [Computer-use research](COMPUTER_USE_RESEARCH.md) — pinned implementation and community review
