# Computer-use and browser-control audit

Audit date: 2026-08-18. Implementation target: version 0.9.0.

This audit separates implemented behavior, benchmark influence, and unimplemented isolation. A delivered input event is never sufficient proof: the target must still match the observation, the authority must still be live, and an application-owned result should be observed whenever the platform exposes one.

## Evidence hierarchy

1. Local source and tests define what this release implements.
2. Live screenshots plus machine-readable results establish that a built artifact exercised that implementation.
3. Chromium/platform source and vendor documentation establish external API behavior.
4. Pinned open-source implementations and papers provide design benchmarks.
5. Community reports suggest failure cases but do not prove platform properties.

The detailed source inventory, pinned commits, empirical ChatGPT/Claude package hashes, and research conclusions are in [COMPUTER_USE_RESEARCH.md](COMPUTER_USE_RESEARCH.md).

## Indicator audit

| Surface | Owner | Version 0.9 behavior | Honest boundary |
|---|---|---|---|
| Chrome debugger warning | Chrome browser chrome | One held `chrome.debugger` attachment keeps **Local Browser Bridge started debugging this browser** visible during the lease | Chrome may suppress the warning for explicit silent-debugger flags or policy-installed extensions; the extension cannot paint or override this UI |
| Page control indicator | Extension content script | Closed-shadow pill states that the bridge is using the tab, shows turn/move counters, and provides a trusted Stop button | It is page content, not Chrome UI; it exists only on scriptable HTTP(S)/file pages |
| Browser synthetic pointer | Extension + CDP | Bounded Bézier/spring movement dispatches real trusted CDP mouse-move events and updates the page pointer before click arrival | It is not the hardware pointer or an OS overlay |
| Computer synthetic pointer | Native helper + image compositor | Session-owned, window-scoped state is moved through exact-window background delivery and composited into returned PNG frames | Version 0.9 has no native click-through desktop overlay; shared frames show settled state, not every intermediate sample |
| Computer live view | Native helper + server | Repeated exact-window observations at 1–10 FPS with monotonic sequence and producer-blocking capture | It is not a virtual display, RDP session, VM, video codec, or separate input seat |

These surfaces are intentionally independent. The Chrome warning proves a debugger attachment. The page pill provides product state and a human Stop action. The helper pointer explains model attention inside returned images. None alone proves end-task success.

## Implementation matrix

| Requirement | Status | Version 0.9 implementation | Required verification |
|---|---|---|---|
| Persistent native Chrome warning | Implemented | Explicit 15-second to 15-minute one-tab lease; five-minute default; held debugger attach; ten-second heartbeat | Real Chrome screenshot after `browser.control.start`, again after multiple actions, and after stop |
| Chrome/user Cancel is authoritative | Implemented | `chrome.debugger.onDetach` hard-revokes state, hides overlay, emits an event, and persists a global human pause that only the extension popup can resume | Cancel during an active session; remote start and mutations on every tab remain refused across restart until popup Resume; no page effect |
| No untrusted click fallback | Implemented | Trusted clicks commit the current target proof and then use CDP; debugger failures propagate | Force detach between proof and dispatch; assert target did not receive DOM `.click()` |
| Human-visible Stop | Implemented | In-page closed-shadow Stop and extension-popup Release both call the privileged lease release path | Click each Stop surface and prove debugger detached and page overlay disappeared |
| Browser action ownership | Implemented | Transport session, control session, observation turn, DOM generation/revision, and pointer move sequence are independently checked | Replay each stale identifier and capture structured refusal |
| Mutation/TOCTOU defense | Partial | Mutation, scroll, and resize invalidation; identity, bounds, visibility, connectivity, and hit-test proof immediately before click | Mutate/occlude/replace targets between observe and action; no click lands |
| One controlled tab | Implemented | Switching lease target detaches old tab first; created tabs enter one named bridge tab group | Start on A, switch to B, prove A is detached and grouped creation is visible |
| Browser pointer arrival | Implemented | Intermediate CDP `mouseMoved` samples, page cursor updates, exact final point, and `moveSequence` metadata | Long and short trajectories at viewport edges; screenshot/trace exact final coordinates |
| Exact-window desktop control | Implemented on supported routes | PID/native-window binding; AX/UIA first; target-routed pixel input; foreground/focus/hardware-cursor/desktop oracle | Per-action application-owned outcome plus unchanged invariant snapshot |
| Computer pointer state | Implemented | Stable helper-session ID/theme, bounded cubic candidate selection, minimum-jerk timing, action state, exact image/screen coordinates, screenshot compositing | Observe before/after move/click/drag/scroll; verify composited tip and monotonic sequence |
| Bounded live frames | Implemented | Exact-window share ID, 1–10 FPS, monotonic sequence, explicit dimensions/scales, producer-blocking capture, latest observation stored by server | Start/status/frame/stop matrix; slow consumer; no unbounded queue growth |
| Renderer acknowledgement/latest-frame-wins | Not implemented | Current producer blocks instead of accepting renderer acks or replacing queued frames | Do not claim agent-browser-equivalent stream freshness |
| Native desktop cursor overlay | Not implemented | Pointer exists only in returned exact-window images | Do not claim Cua-equivalent click-through overlay visibility |
| Full PUSV visual atomicity | Partial | Structural/geometry/window revalidation, but no target-patch SSIM or full-frame pre-dispatch diff | Treat custom/canvas UI meaning changes as a remaining risk |
| Isolated OS input session | Not implemented | `background-window` preserves foreground and cursor but shares the user's login session | Use a managed VM/RDP/separate login for hostile or destructive workloads |
| Protocol replay boundary | Implemented | Server-first welcome; exact package/protocol/session match; monotonic command/event sequences; exact result echo; bounded queues | Wrong version, session, sequence, duplicate event, reconnect, and queue-saturation tests |

## Benchmark delta

| Benchmark | What it does better | What version 0.9 adopts | Remaining delta |
|---|---|---|---|
| Cua Driver `c43f102` | Native cross-platform click-through cursor, Dubins planner, velocity/spring renderer, themes and session badges | Session ownership, stable visual identity, bounded curved path, action state, explicit target/coordinates, composited evidence | No native overlay; different bounded cubic/minimum-jerk planner; no claim of pixel-identical or behavior-equivalent renderer |
| agent-browser `548b159` | Persistent browser runtime, latest-frame-wins, per-viewer FPS, optional one-frame acknowledgement pacing | Persistent control session, monotonic frame sequence, bounded FPS and queue | No viewer ack or latest-frame replacement; exact-window PNG feed is smaller in scope |
| pi-computer-use `de7258` | Immutable state store, per-resource epoch scheduler, checked multi-action transactions, successor diffs, optional native ghost cursor | Layered stale IDs, exact-window identity, serialization, post-action observe, fail-closed uncertainty | No general resource-epoch transaction engine, successor diff system, or native ghost cursor |
| ChatGPT public CRX snapshot | Exclusive tab/session lifecycle, per-target ordering, sophisticated cursor/arrival protocol, managed group | One-tab lease, turn/move state, serialization, managed group, held debugger attachment | Independent protocol and implementation; no claim of feature parity with a later package |
| Claude public CRX snapshot | Clear indicator/Stop/heartbeat product surface | Page pill, trusted Stop, heartbeat, closed-shadow isolation | Different UI and protocol; no claim that page overlay is native Chrome UI |
| UFO² paper | Separate RDP-loopback input session with PiP viewing | Explicit future `isolated-session` seam only | Not shipped by this project; reviewed UFO source did not establish that the paper mode was available |
| PUSV paper | Immediate layered pixel/window recheck with measured attack coverage | Structural + exact-window preflight and refusal | No SSIM/global frame defense and no measured PUSV attack benchmark |

“SOTA” is therefore a vector, not a release badge. Version 0.9 materially upgrades controller visibility, authority lifetime, stale-state refusal, pointer evidence, and bounded viewing. It is deliberately behind native-overlay projects on physical-desktop visualization and behind RDP/VM designs on security isolation.

## Release evidence matrix

The checked-in [version 0.8 evidence](../evidence/v0.8.0/README.md) proves the earlier exact-window macOS fixture, semantic actions, browser extension load path, and non-interruption baseline. It does not prove the new version 0.9 lease, indicator, protocol, pointer, or share behavior.

Before publishing version 0.9, create a separate `evidence/v0.9.0` bundle containing sanitized screenshots and machine-readable results for at least:

| Scenario | Visual evidence | Machine-readable evidence |
|---|---|---|
| Real unpacked extension load | `chrome://extensions` showing Local Browser Bridge version 0.9.0 | Manifest/package hash and extension contract output |
| Browser lease start | Native Chrome warning and separate page pill/Stop | `browser.control.start` state with session, tab, expiry, turn, and pointer sequence |
| Long pointer move + click | Page cursor path/final position and resulting demo state | Motion duration/point count/arrival and trusted click result |
| Chrome Cancel | Warning disappearing and no target effect | `canceled_by_user` revocation plus refused next action |
| Page mutation and occlusion | Mutated/covered test target | `STALE_SNAPSHOT` or `TARGET_CHANGED` with no click effect |
| Protocol mismatch/replay | Control surface incompatible state | Exact version/session/sequence error and zero effective capabilities |
| Computer share lifecycle | Exact-window frame with settled synthetic pointer | Start/status/frame sequences/stop and explicit capture/coordinate metadata |
| Each helper action | Before/after exact-window PNG where meaningful | Frame ID, delivery mode, invariants, action outcome, and fresh successor frame |
| Non-interruption | Foreground application remains the user's app | Foreground, focus, hardware cursor, and active Space/Desktop unchanged |
| Release artifacts | None required | SHA-256 manifest, platform/architecture inspection, GitHub attestation verification |

Screenshots that expose personal tab titles, URLs, accounts, tokens, or unrelated windows must stay in ignored local output. Only dedicated test windows and sanitized crops belong in the repository.

## Remaining boundaries

- Chrome's native warning depends on an actual non-suppressed `chrome.debugger` attachment. A screenshot of only the page pill does not prove it.
- Chrome's Cancel action can terminate all debugger clients owned by that extension. Version 0.9 intentionally holds only one controlled-tab attachment.
- The page overlay cannot run on restricted `chrome://`, `edge://`, extension, or protected pages. Those pages also remain outside extension control.
- Open shadow roots are covered; cross-origin iframe semantic merging is not. CDP child-target support should be added and tested before claiming cross-origin semantic parity.
- The browser extension has broad HTTP, HTTPS, and optional file authority in Full Access. Loopback authentication, stale-state checks, and visible control do not make prompt-injected page content trustworthy.
- macOS background delivery depends on private SkyLight interfaces. Unsupported OS/framework combinations must refuse rather than switch to global input.
- Windows UIA compilation is not representative Windows runtime proof. A native Windows fixture remains required before a new UI framework/action combination is marked supported.
- Exact-window sharing prevents unrelated-window capture but does not isolate memory, credentials, app state, or OS permissions from the user's session.

## Acceptance rule

Do not mark version 0.9 deployment complete until the release-specific evidence exists, the full Rust/extension contract suite passes, both target artifact sets are verified, and the real-Chrome Cancel path proves fail-closed behavior. A transport-level success or a single screenshot is diagnostic evidence only.
