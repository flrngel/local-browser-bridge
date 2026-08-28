# Development

End users do not need Node.js. The entry points are Rust executables; the macOS build links a bundled Swift ScreenCaptureKit bridge and system frameworks. Developers running the complete contract suite need Node.js 24 for the checked-in extension and dashboard behavior harnesses, and Unix-only contract helpers also use Bash and Python 3. None of those test tools is packaged into or invoked by the server, helper, or extension. See [Building from source](BUILD.md) for platform setup, native commands, output paths, and package construction.

## Prerequisites

- Rust 1.88 or later with Cargo, rustfmt, and clippy
- Node.js 24 for extension/dashboard behavior contracts and browser evidence rigs
- Bash and Python 3 for the complete contract suite on Unix
- Platform SDK and linker for the target operating system. Native macOS builds
  require the macOS 26 SDK or newer because the locked `apple-metal` Swift
  bridge names Metal 4 APIs at compile time. The resulting universal package
  still targets and is artifact-checked for macOS 13 on both architectures.
- `zip` and `unzip` for extension packaging
- Chrome or Edge 140+ for browser and recursive cross-origin iframe testing
- macOS 13+ with Screen Recording and Accessibility permissions for native macOS testing
- A signed-in interactive Windows 11 session for native Windows testing

Use the exact versions in `Cargo.lock`. Distributed versions in `Cargo.toml`, `Cargo.lock`, `extension/manifest.json`, and `extension/lib.js` must remain aligned.

## Repository layout

| Path | Purpose |
|---|---|
| `src/` | Rust server, protocol, update check, and computer helper code |
| `extension/` | Manifest V3 Chromium extension |
| `public/` | Control page and local demo embedded into the server |
| `tests/` | Rust contract and integration tests |
| `evidence/` | Versioned live-run results and screenshots |
| `packaging/` | Platform packaging metadata |
| `scripts/` | Version, package, deploy, and artifact verification scripts |
| `docs/` | Protocol, architecture, research, capability, and limitation references |

## Build

Use the platform-specific [building guide](BUILD.md). It distinguishes native
development binaries from the Windows release shape and the universal macOS
server/helper app bundle. The server embeds its UI; do not add a runtime
dependency on a JavaScript package manager, remote script, or CDN.

## Local protocol test

Start the server:

```bash
cargo run --locked --release
```

In a second terminal, run the Rust mock connector:

```bash
cargo run --locked --release --bin mock-extension
```

Open the complete authenticated control-page URL printed by the server. The server and mock connector read the same generated token file unless `LBB_TOKEN` or `LBB_TOKEN_PATH` overrides it.

### Runtime configuration

| Variable | Default | Purpose |
|---|---|---|
| `LBB_PORT` | `17373` | Loopback HTTP/WebSocket port |
| `LBB_TOKEN` | Generated automatically | Explicit bridge token |
| `LBB_TOKEN_PATH` | Computed profile path | Generated-token storage path; startup never falls back to the working directory when the profile is unavailable, and any explicit/non-default parent must already exist with private current-user-only permissions and is never rewritten |
| `LBB_DISABLE_UPDATE_CHECK` | `false` | Disable the one-time GitHub metadata check |

The equivalent `--no-update-check` flag disables the startup check; `--check-updates` performs the metadata-only check and exits. The server still binds only to loopback.

## Test the real extension

1. Open `chrome://extensions` or `edge://extensions`.
2. Enable Developer mode.
3. Select **Load unpacked** and choose `extension/`.
4. Enter the server token and port in the extension popup.
5. Use `/demo` for ordinary element, key, scroll, dialog, and navigation checks.
6. Confirm the Chrome-owned debugger warning, page pill, trusted Stop, revocation, and reconnect behavior.

Use Chromium 140 or later for the nested cross-origin fixtures. A passing same-process iframe test does not prove the OOPIF route.

## Test the computer helper

Run the helper in a third terminal:

```bash
cargo run --locked --release --bin local-computer-helper
```

On macOS, request permissions through the packaged app identity when testing release behavior. A raw `cargo run` process is useful for development but is not evidence for the packaged TCC flow.

Test one-shot observation and persistent sharing as separate lifecycles:

- On macOS, `computer.observe` exercises the snapshot backend. On Windows, it starts the same bounded WGC implementation as live sharing, consumes one fresh frame, and proves shutdown.
- `computer.share.start` exercises ScreenCaptureKit `SCStream` on macOS or Windows Graphics Capture on Windows.
- Share tests must cover start, first useful frame, monotonic sequences, bounded replacement, dropped-frame accounting, target closure, explicit stop, and connector replacement.
- Input tests must keep three layers distinct: sealed exact-target route provenance, operating-system API acceptance where a signal exists, and an application-owned postcondition. Only the postcondition can confirm target effect.
- `cursorPositionUnchanged` is diagnostic, not action-source authority. Tests must assert `helperGlobalPointerPreservation`, `sharedPointerBoundaryCorroborated`/`sharedPointerBoundaryState`, `hidSystemPointerActivityObserved`, `pointerActivityMonitorHealthy`, and `sharedPointerActivityState` according to the platform contract. HID-system activity is never physical-device provenance.

Do not report cross-Space, minimized, protected, elevated, or framework-specific behavior as supported merely because a stream or message API returned success.

The v0.12.30 candidate passed both packaged macOS lanes, then failed closed
before the Windows runner launched because the production staged
worker-support loader referenced an undefined hex helper. Version 0.12.31 fixed
that loader and passed its quiet macOS lane, but its deliberate lane failed
closed before product dispatch when the bounded post-`ACTION`
fresh-share-frame refresh timed out. Windows candidate execution, stock Chrome,
tagging, and publication did not run for either candidate. Version 0.12.32
replaced that refresh with a larger bounded, abortable authority wait. Its one
candidate passed source, provenance, package, architecture, and signature
checks, then failed before permission probes, quiet-seat stabilization, or any
candidate executable ran because the source-compiled app-share self-test emitted
an extra diagnostic line outside its exact one-line output contract. The
complete two-file negative record is retained on the evidence-only branch
[`evidence/v0.12.32-macos-app-share-handoff-self-test-failure-32937681956`](https://github.com/flrngel/local-browser-bridge/tree/9bece01635e4296fbb3ff0f3651100245c5d7729/evidence/v0.12.32/computer/attempts/withdrawn-05c565c-macos-app-share-handoff-self-test-failure).
Windows, stock Chrome, tagging, and publication did not follow. Version 0.12.33
retained the deadline assertions, restored the single success line, and made
both CI and candidate packaging enforce the same stdout, stderr, and exit-status
contract. Its fresh quiet and deliberate-concurrency macOS lanes passed. Its
one Windows attempt then failed closed at `build-dedicated-fixture` before a
fixture executable or candidate product process existed. Version 0.12.34 let
runner-owned children explicitly break away from both the detached-worker guard
Job and named lifetime Job, then atomically bound each child to the runner's
private Job at creation. Its exact trust gate and both fresh macOS lanes passed.
The Windows coordinator then created its persistent no-retry reservation at
`2026-08-26T11:05:46Z`, before any coordinator state existed, and the invoking
session was interrupted. A bounded later observation found no v0.12.34
coordinator directory, evidence directory, candidate process, listener on port
17373, Computer Use action, or Chrome action. That absence is only a bounded
observation; it cannot prove that an unobserved transient state never existed.
The ledger therefore leaves the Windows attempt `not-started` but not retryable.
Version 0.12.34 was withdrawn without a tag or GitHub Release.

Version 0.12.48 retains the nested-Job repair and moves the persistent attempt
boundary to the final pre-launch sequence described below. Its source-only
regression launches the actual guarded worker, adds the lifetime Job, and
compiles and executes the fixture through that topology. It must use fresh
source-bound artifacts and evidence; no earlier candidate result may be
relabelled or reused.

The sole v0.12.46 candidate passed 68 of 69 macOS quiet-lane assertions, then
failed closed before product dispatch because its independent active-receiver
watcher expired after eight seconds while the server's command boundary could
remain live for fifteen seconds. The fixture click count stayed zero and the
user foreground, AX focus, cursor, Space, and HID boundaries remained safe.
No deliberate macOS, Windows, stock-Chrome, tag, or Release step followed.
Version 0.12.48 uses one sixteen-second deadline in both the runner request and
the native SystemProbe cap, and locks that binding in the Rust source contract.

The sole v0.12.47 candidate then passed both packaged macOS lanes and mandatory
review of all twelve fixture-only screenshots. The coordinator combined the
documented sequential aggregate-directory assignments into one `export`
command under `set -u`, so the finalizer received an empty output path and
failed closed. Its exact sanitized lane record is retained under
`evidence/v0.12.47/computer/attempts/withdrawn-6f469e4-macos-finalizer-empty-path/`.
Windows, stock Chrome, tagging, and publication did not start. Version 0.12.48
adds `scripts/finalize-macos-acceptance.sh`, which exclusively owns the
dependent directory assignments and returns the canonical aggregate path only
after successful finalization.

The v0.12.48 macOS harness defines two fresh, non-mergeable release lanes in [`evidence/v0.12.48/computer/README.md`](../evidence/v0.12.48/computer/README.md). Both require a healthy monitor, unchanged sampled cursor, no shared input activity, and `sharedPointerActivityState: quiet` for every evidence cell. The `deliberate-concurrency` compatibility lane adds a separately authorized exact-app-share button action and proves—through a target-owned request/start/complete chain plus independent bundle/window/button and shared-seat probes—that app-scoped orchestration spanned the real product action without using the shared desktop. It does not claim physical-human or cryptographic Computer Use provider identity. Never convert, merge, or substitute optional physical-pointer adversarial bytes into either release lane. An unknown monitor or boundary fails closed.

Before either lane invokes a candidate binary—even with `--version`—the exact
source-bound SystemProbe must complete a 30-second native quiet-seat epoch with at
least 60 stable transitions at 500 ms cadence. Any cumulative pointer/cursor,
foreground, AX focus/front-window, or active-Space change resets all epoch
progress under the original 30-minute deadline. Unknown or unhealthy monitoring
is immediately fatal. The runner records only bounded summary facts, starts the
fixture/server/helper after completion, and keeps every later per-action and
whole-run proof unchanged. The deliberate lane uses this gate before its
exact-app-share request; no shared-pointer movement is required or accepted.

The sole v0.12.35 candidate reached this macOS gate and failed closed before
candidate process execution after repeated shared-seat resets and an unknown
native-monitor sample. No Windows, stock-Chrome, tag, or Release followed; its
immutable sanitized negative record is linked from the v0.12.48 harness
README. Version 0.12.48 does not relax that gate. It adds fixed reset/unknown
cause categories and a source-only scheduling check:

```bash
node evidence/v0.12.48/computer/helper-evidence-rig.mjs --quiet-readiness
```

That command accepts no candidate paths, emits one sanitized JSON record, and
always reports `candidateInvocations: 0` and `acceptanceEvidence: false`. A
ready exit is not release evidence and cannot satisfy or replace either
candidate-bound lane.

For the deliberate-concurrency lane, start the packaged evidence runner and then run `node scripts/wait-macos-app-share-concurrency-handoff.mjs --mode watch --evidence-dir "$ABSOLUTE_EVIDENCE_DIRECTORY" --runner-pid "$RUNNER_PID"` in a separate terminal. The read-only watcher waits for the exact runner-created lane, opens only the request/start/complete records without following links, validates the v0.12.48 marker-schema-2 records, stable app surface, request hash, start-receipt hash, canonical timestamps, and process binding, and writes nothing. The candidate identity nested in the lane is the separate schema-3 binding. On `ACTION REQUIRED`, use the separately authorized exact-app share for bundle `dev.flrngel.local-browser-bridge.acceptance.app-share`, press `START APP-SHARE CHECK` exactly once, do not use the shared desktop or retry, and stop all UI use after `START RECEIVED`. The app independently verifies the request hash before writing its create-once start receipt and disabling the button. After that receipt, the runner uses a bounded, abortable authority refresh to obtain a strictly newer streamed frame from the same share, exact target, and unchanged geometry within the reserved handoff deadline; only that fresh frame may authorize the product click. It then proves unchanged foreground/focus/Space/cursor/HID state, dispatches the real bounded product action, requires the target postcondition and quiet product/independent boundaries, and completes the app-owned receipt chain. A copied, stale, changed, or missing record, duplicate action, unknown native boundary, dead runner/app, shared-seat activity, frame-refresh failure, or timeout fails closed. The marker chain and watcher are orchestration evidence, never product authority. Validate with `node --check scripts/wait-macos-app-share-concurrency-handoff.mjs`, `node scripts/wait-macos-app-share-concurrency-handoff.mjs --mode self-test`, `node --check evidence/v0.12.48/computer/helper-evidence-rig.mjs`, `node evidence/v0.12.48/computer/helper-evidence-rig.mjs --self-test`, and Swift typechecks for `AppShareHandoff.swift`, `PhysicalPointerHandoff.swift`, and `SystemProbe.swift`. The old pointer watcher and physical prompt remain optional adversarial tooling and cannot satisfy release.

### Deterministic Windows live acceptance

Version 0.12.29 has completed the Windows-native coordinator source gate
described in the [Windows acceptance source-gate record](WINDOWS_ACCEPTANCE_HANDOFF.md).
The exact PowerShell 5.1 parser and all eight GUID-scoped non-product native
scenarios passed, and independent review found no P0/P1 issue. This result does
not authorize the live commands below by itself: no packaged 0.12.29 candidate
has been built, downloaded, or executed, and no macOS, Windows candidate,
stock-Chrome, tag, or Release gate has occurred. The commands remain the future
candidate procedure after one exact artifact set is frozen and verified.

`tests/fixtures/windows/WindowsComputerUseFixture.ps1` is the source for an animated target with a same-thread capture-evidence backdrop, a foreground sentinel on its own UI thread, and an optional magenta occluder on another UI thread. The target exposes a UI Automation `ValuePattern` edit and `InvokePattern` button, a retained-focus text edit, and a custom surface that records mouse, drag, wheel, `WM_KEY*`, and `WM_SYSKEY*` messages without recording character contents. Its state stores lengths and SHA-256 proofs instead of typed text.

Live acceptance does not host that UI in PowerShell or a terminal. The runner hashes the exact fixture source, creates one random runner-owned directory under the ordinary system temporary directory, and invokes its own exact system Windows PowerShell 5.1 Desktop image to compile the source as a C# `WindowsApplication`. Build mode requires that source SHA-256 and checks it both before and after compilation. The runner records the bounded executable size and SHA-256, executes its strict entry-point self-test, verifies that the executable hash did not change, and launches the same `.exe` directly rather than passing live fixture arguments to a PowerShell console host. The executable is ephemeral acceptance tooling and never enters `dist`, a workflow artifact, retained evidence, or a Release.

The dedicated entry point accepts only exact, case-sensitive `--self-test` or `--evidence-directory DRIVE_ABSOLUTE_NON_ROOT_PATH [--show-occluder]`. Drive-relative, root-relative, drive-root, relative, and UNC paths are refused, as are reordered, duplicated, unknown, or differently cased arguments and any evidence directory that already contains a protected fixture record. Before opening UI it sets the stable AppUserModelID `LocalBrowserBridge.WindowsAcceptance`. That AUMID is discoverability metadata for an app-share picker, not process identity, consent, click provenance, or acceptance authority.

The runner establishes the actual GUI identity independently: the live process image must equal the just-built executable, its session must equal the runner's signed-in interactive session, the ready-file PID must equal that live process, and it must be the runner's sole exact-image direct child. The executable hash must still match at the initial live image binding. These checks precede server startup. The sanitized schema-2 result summary retains them under `fixtureProcessBinding`, while its `releaseCandidateBinding` is schema 3. It includes source/executable hashes and stability, entry-point self-test, exact image/session/direct-child/ready-PID matches, `executionMode: dedicated-windows-application`, `terminalHostUsed: false`, and eventual executable removal; it retains no executable path.

The checked-in coordinator runs `scripts/test-windows-computer-use.ps1` exactly once from a signed-in interactive Windows session. Use a dedicated test session. After the helper, recovery event, and exact target binding are ready, the runner posts a fresh test-only generation to the fixture. It waits for a separate bounded receipt proving that exact generation was processed once, the exact same-thread fixture-owned root/button topology still holds, the orange sentinel has enabled its large **CLICK TO ARM** button, and input is either untouched or already one internally consistent acknowledgement. This second form handles a valid click made as soon as the button visibly enables. The sentinel is initially shown without activation; the deliberate click can still activate and focus it normally. Its top-level title is the exact stable string **LBB Foreground Sentinel** from creation through waiting, action-required, and armed states; only the in-window status label and button text change. The runner saves the delivery proof, atomically creates the runner-create-once notification `operator/foreground-arm-request.json`, and prints `ACTION REQUIRED`: click only if the button still says **CLICK TO ARM**; if it already says **ARMED**, do not click again. A human or separately trusted Computer Use surface must left-click that button once and stop using the session. The fixture accepts the mouse down only while native global foreground equals the sentinel and the calling UI thread's focus equals the exact button; focus loss, deactivation, or a new generation clears the pending press. Mouse up must repeat the exact generation, button, point, foreground, and focus checks before acknowledgement. Total button left-down/up attempt counts must each remain exactly one. After the click and three stable native samples, the runner atomically creates the matching `operator/foreground-arm-received.json`; it still requires an exact arm-to-baseline continuity check before any product action. Both operator files are sanitized, create-once notifications. The runner never reads them and they are never acceptance authority, so a copied, stale, changed, or replayed marker cannot satisfy the click proof. Before the first baseline command, it re-binds the original authenticated helper session and PID to the same exact-image direct child and completes another `computer.status` round trip; a worker restart during the arm wait fails closed. It never calls `SetForegroundWindow`, synthesizes an Alt key, or loops to steal focus. This is a trusted interactive-session receipt, not cryptographic proof that a physical human produced the window messages; the acceptance session must exclude untrusted same-integrity injectors. The explicit setup boundary makes subsequent background-control invariants deterministic without treating `TopMost`, `Shown`, or a WinForms activation callback as global-foreground proof. The runner requires the intended version, the frozen candidate's exact `SHA256SUMS.txt`, its SHA-256 recorded independently by the release coordinator, canonical server/helper filenames, fixture, evidence directory, and ephemeral token. Before launch it requires the manifest's exact four-asset inventory, matches both executable hashes, checks both VERSIONINFO versions, and checks bounded `--version` output. It never installs software, changes security or network settings, dismisses warnings, or terminates processes it did not start. It refuses files carrying Windows download-zone metadata instead of unblocking them or bypassing a Windows warning.

Live mode intentionally runs only under the system Windows PowerShell 5.1 Desktop host because compilation and the acceptance fixture use WinForms/.NET Framework. PowerShell 7 remains a parser and self-test surface. The checked-in coordinator resolves the native Windows system directory without depending on a Machine-scoped `SystemRoot`, re-enters its exact 64-bit Windows PowerShell host through a nonce-bound clean bootstrap, and requires identity `5.1|Desktop|True` before any candidate launch. The compiled GUI is nevertheless launched as its own Windows-application process, not as terminal-hosted PowerShell UI.

```powershell
$version = "0.12.48"
$server = (Resolve-Path ".\dist\local-browser-bridge-v$version-windows-x86_64.exe").Path
$helper = (Resolve-Path ".\dist\local-computer-helper-v$version-windows-x86_64.exe").Path
$manifest = (Resolve-Path .\dist\SHA256SUMS.txt).Path
$candidateBinding = (Resolve-Path .\candidate-binding.json).Path
# Copy this value from the trust gate's independently recorded frozen-candidate inventory.
$manifestSha256 = "EXPECTED_64_CHARACTER_SHA256"
$fixture = (Resolve-Path .\tests\fixtures\windows\WindowsComputerUseFixture.ps1).Path
$localAppData = [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)
$evidenceParent = Join-Path $localAppData "LBB-Windows-Acceptance-Evidence-v1"
$coordinatorParent = Join-Path $localAppData "LBB-Windows-Acceptance-Coordinators-v1"
$runNonce = [Guid]::NewGuid().ToString("N")
$evidence = Join-Path $evidenceParent ("acceptance-" + $runNonce)
$coordinator = Join-Path $coordinatorParent ("acceptance-" + $runNonce)

& .\scripts\run-windows-computer-use-acceptance.ps1 `
  -Mode Start `
  -Version $version `
  -ServerPath $server `
  -HelperPath $helper `
  -ChecksumManifest $manifest `
  -ChecksumManifestSha256 $manifestSha256 `
  -CandidateBindingPath $candidateBinding `
  -FixturePath $fixture `
  -EvidenceDirectory $evidence `
  -CoordinatorDirectory $coordinator `
  -ForegroundArmTimeoutSeconds 300
```

`Start` enters an exact clean system-PowerShell bootstrap even when its caller is
already PowerShell 5.1, creates and revalidates the empty evidence directory,
and protects the coordinator directory with an exact owner-only ACL. It returns
only after its private retained worker publishes an atomically moved create-once
start record and the exact worker PID/start time is still live. The worker
inherits only an explicit ordinary Windows environment allowlist, generates the
bearer token internally, places it only in the exact runner child environment,
and clears it immediately. One session-wide mutex excludes an overlapping
coordinator and is acquired before stable named-Job recovery. One monotonic
deadline spans opening and terminating the exact prior Job, querying that handle
until `ActiveProcesses == 0`, closing it once, and polling until the name leaves
the namespace. The worker then clears last error, calls `CreateJobObject`
exactly once, and accepts only a non-null handle with last error zero. Every
nonzero status, including `ERROR_ALREADY_EXISTS`, closes the returned handle and
fails without adopting, terminating, configuring, retaining, or retrying the
uninspected object. It configures `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and binds
itself before publishing Worker state; Intent therefore also follows binding.
Runner, watcher, and worker stdout/stderr are separate persistent files in the
owner-private coordinator directory, never pipes that an external wrapper can
abandon. The worker writes launch intent before `Process.Start()`, records the
exact runner PID/start time after success, starts exactly one read-only watcher
only after the atomic foreground-arm request marker exists, and remains alive
through the runner's final exit. It never initializes or drives UI.

Poll the same coordinator directory with the read-only `Follow` mode:

```powershell
& .\scripts\run-windows-computer-use-acceptance.ps1 `
  -Mode Follow `
  -CoordinatorDirectory $coordinator
```

Only a failure proved to precede the persistent schema-2 reservation is
`not-started`. Once that reservation exists, even if `runner-launch-intent` is
absent, the outcome is terminal `candidate-execution-unknown`. Launch intent
without a conclusive start result is likewise outcome-unknown. A successful
`Process.Start()` consumes the one candidate attempt even if the runner writes no
product evidence. The coordinator itself creates and revalidates the empty
evidence directory before worker launch, so directory existence proves neither
candidate execution, liveness, nor acceptance. A watcher or runner failure is
terminal and must never be retried. A
started runner without a final summary, or with a missing/false summary pass bit,
is also terminal; retain the private coordinator diagnostics for sanitized
review. `Follow` prioritizes the first terminal failure record over a later final
record and refuses both waiting and handoff output unless the exact worker
PID/start time is still live.

The watcher reads only `operator/foreground-arm-request.json`. It requires the
exact v0.12.48/schema-2 marker field set and order, a fresh non-expired publication,
ordinary non-reparse paths, and the same live runner PID/start time both before
and after parsing. It emits exactly one compact sanitized
`foreground-arm-visual-handoff` JSON object or fails closed. It neither writes
evidence nor reads product state, moves focus, injects input, or verifies the
separate external authorization.

Treat that output as routing, not consent or authority. The coordinator
preserves the watcher's non-authority fields, keeps `uiActionAllowed: false`,
and requires an external consumer to deduplicate the stable `requestId`; calling
`Follow` repeatedly never creates a new authorization. Only after the watcher
has validated the fresh marker—and therefore after the dedicated sentinel
exists—initialize the separately authorized Windows Computer Use app share and
list its windows. Do not initialize, prewarm, reserve, or cache the app-share
inventory before the current run creates that GUI and publishes its marker. The
stable `LocalBrowserBridge.WindowsAcceptance` AUMID can improve discovery but
cannot bind the selected surface to the runner.

Obtain a fresh one-shot authorization for the named external surface, observe a
fresh shared frame, and left-click exactly once only if it visibly shows the
exact orange **LBB Foreground Sentinel** window and **CLICK TO ARM** button. That
top-level title is identical in waiting, action-required, and armed states; use
the button text for the state decision. If the frame shows **ARMED**, is stale,
or is ambiguous, perform zero clicks. Do not make a preparatory focus click, use
a keyboard/UIA/scripted substitute, or retry an unknown outcome. Stop all UI
interaction immediately after the one action and poll only runner liveness, the
matching received notification, and final `summary.json`. Neither notification
nor the AUMID can satisfy acceptance; only the fixture's exact click/native
proof, bound fixture process facts, and final summary/inventory can pass. Keep
watcher and runner stdout/stderr outside the evidence directory and delete those
coordinator-owned files after sanitized review.

Exercise the watcher parser, liveness/freshness refusals, action-required
one-click handoff, and already-armed zero-click handoff without opening UI:

```powershell
powershell.exe -NoLogo -NoProfile -File `
  .\scripts\wait-windows-foreground-arm-handoff.ps1 -Mode SelfTest
```

The expected sole output is `Windows foreground-arm handoff watcher self-test passed.`
CI and the protected release workflow run that self-test both with `-File` and
by creating and invoking the watcher script block inside the exact system
Windows PowerShell 5.1 process, so the live call-operator function-resolution
boundary is exercised rather than inferred from a parser pass.

The coordinator accepts new evidence and coordinator directories only as direct
children of those two fixed owner-private LocalAppData parents on a local
NTFS/ReFS volume. It rejects Temp paths, UNC paths, links, reparse traversal,
and caller-selected alternate parents. Version 0.12.48 resolves the prospective
ledger identity without creating it, then completes owner-private directory and
ACL setup; stages and rehashes the exact scripts, fixture, manifest, binding,
and candidate executables; publishes and verifies the private configuration and
Start records; binds the detached worker to its fresh lifetime Job; transfers
guard ownership; and prepares the exact runner process, arguments, environment,
and ephemeral token. Only then does the worker atomically create the persistent
per-version no-retry reservation. That create-once boundary immediately
precedes the runner launch-intent record and process creation. A failure before
the boundary may leave diagnosable private scratch state but does not consume
the candidate attempt. Once the ledger exists, treat the candidate outcome as
unknown unless later terminal evidence resolves it more narrowly: never delete
the ledger or retry that product version. A coordinator result with
`retryAllowed: false` forbids automatic retry and any replay or re-entry by the
current coordinator; it does not, by itself, claim that every future
coordinator is globally forbidden. A separately authorized fresh `Start` is
possible only for a failure proved to be before the reservation boundary, after
independent confirmation that the per-version ledger is absent and cleanup of
all prior coordinator-owned runtime resources, including processes and
listeners, is complete. Retained immutable diagnostics grant no retry
authority. Any missing, invalid, or ambiguous proof fails closed.

Also run the persistent coordinator's non-product self-test through exact system
Windows PowerShell 5.1:

```powershell
& .\scripts\run-windows-computer-use-acceptance.ps1 -Mode SelfTest
```

Its sole success line is `Windows computer-use acceptance coordinator self-test passed.`
The test covers exact owner-only ACLs, worker Job Object binding, Win32 argument
quoting, rapid nonzero child exit, simultaneous large stdout/stderr retention,
flush-before-atomic-move create-once publication, exact-byte worker-support
loading, explicit detached-worker environment isolation, bearer-token absence
from argv/logs/state, exact-tick process identity, and the process-lifetime
stable session-wide mutex. Its eight GUID-scoped native lifetime scenarios prove:

1. clean named-Job creation and binding;
2. recovery of an exact live prior owner and descendant;
3. waiting for a delayed non-member handle and namespace disappearance;
4. bounded fail-closed timeout while an extra handle outlives the deadline;
5. refusal of a raced same-name object at the single final create;
6. exact worker termination when the pre-transfer guard closes;
7. worker survival after guard transfer, followed by Job-owned descendant
   termination without affecting an unrelated control process; and
8. unlockable stdout/stderr plus deletion of only self-test-owned files.

It also covers waiting-state UI refusal, live/dead worker handling,
byte-identical repeated non-authoritative handoff output with request-ID
deduplication, exact predecessor-chain validation, and terminal-failure
precedence. The exact Windows PowerShell identity was `5.1|Desktop|True`, and
independent review found no P0/P1 native-handle, Job-identity, deadline, cleanup,
or publication-gating issue. It opens no candidate, browser, network, or UI
surface.

These create-once records survive a dropped SSH shell and ordinary coordinator
process failure, but the script does not claim sudden-power-loss durability for
the directory-entry rename. If storage or machine failure makes any reservation
or terminal state ambiguous, treat candidate execution as outcome-unknown and
do not retry that product version.

The fixture increments a monotonic publication generation on every state write. Arm stability consumes three distinct advancing publications after the delivery receipt, the baseline consumes another, and every later invariant comparison requires a still-newer publication. Replaying one valid state file or stalling the fixture writer therefore cannot satisfy the acceptance oracle. The stable AUMID and app-share discovery outcome are intentionally absent from that oracle.

The independently runnable suites are `Smoke`, `Recovery`, `Semantic`, `Keyboard`, `Pixel`, `Capture`, and `Cancellation`; pass more than one as `-Suite Semantic,Pixel`, or use `All`. `Recovery` requires `-TimeoutSeconds 25` or greater; the default is 45. Foreground arming has its own monotonic 15–300 second bound and defaults to a bounded 300-second runner arm interval; product request-delivery and command timeouts are still separate and do not expand. The separate request-delivery phase is bounded to ten seconds. Sanitized proofs record request/ack generation equality, exact button identity, native same-thread owner/root/child topology, request and acknowledgement counts, exactly-one left-down/up attempt counts, button-enabled state, stable-sample count, arm-to-baseline continuity, and timeout—never raw window handles or cursor coordinates. A successful `-Suite All -ShowOccluder` run retains exactly 88 files: three fixture records, 62 step records, 20 sanitized screenshots, two operator notifications, and `summary.json`; the dedicated executable and its binding facts add no file because the facts are nested in `summary.json` and the executable is removed. A pre-click timeout retains only the request marker and the files reached before failure. `Capture -ShowOccluder` verifies animated native-frame progression, exact-window occlusion exclusion, stream progression across an action, explicit stop, and the sanitized desktop-level indicator proof described below. `Cancellation` starts a real two-second target-routed move, waits for the fixture-owned `WM_MOUSEMOVE` counter instead of assuming dispatch from a timer, proves duplicate suppression, authenticated cancel, cached outcome-unknown replay, changed-request refusal, old-worker replacement, screenshot removal, late-frame quarantine, a replacement-session `NO_COMPUTER_FRAME` refusal because normalized coordinates have no observation dimensions before explicit observe, explicit one-shot observation recovery on the replacement worker, continued old-frame staleness, and a fresh recovered action. Unlike the macOS same-session proof, replacement attach clears the old session's authority gate; the Windows refusal proves missing replacement-session frame authority, not an inherited revocation gate. Every action suite saves exact-target screenshots plus app-owned result/state evidence and compares the foreground HWND, focused HWND, OS-global cursor position, input desktop, target activation count, sentinel deactivation count, arm generations, arm request/ack counts, and arm-button input-attempt counts before and after delivery.

`Recovery` is a launch-time-only fault proof, not a remotely callable protocol capability. The runner gives the helper supervisor a unique validated `Local\\LBBTestSharePump-*` manual-reset kernel-event name through `LBB_TEST_STALL_SHARE_PUMP_ONCE_EVENT`. The supervisor creates and holds the initially unsignaled event before launching a worker. The first disposable Windows worker signals it and stalls its first active-share conversion task; the worker's hard deadline emits a best-effort `COMPUTER_HELPER_WATCHDOG` error and exits, while the supervisor remains alive and replaces it. The suite requires the event to become signaled, a new direct-child worker PID and helper session ID, unchanged supervisor and server PIDs, continuously successful server-state polls, then a fresh exact-window observation and live native-share frame from the replacement worker. Replacement is accepted only when the server exposed `COMPUTER_HELPER_WATCHDOG` or at least 11.5 seconds elapsed after the runner observed the signaled fault event, providing a conservative timing lower bound for the 12-second watchdog; both share-start and event-relative durations are retained. Later workers observe the supervisor-held signaled event and do not stall. Closing the private Job releases the event; the runner verifies that it no longer exists. The hook neither creates a file nor adds a remotely triggerable protocol method.

The manifest SHA-256 is an out-of-band binding value: do not derive it from the same untrusted copy immediately before the run. The release coordinator records it when downloading the exact gated workflow artifact and sends that value with the candidate. The runner also requires the trust wrapper's create-once `candidate-binding.json`, validates its exact source, `refs/heads/main` source ref, workflow run and attempt, artifact, raw-ZIP, attestation, and five-file facts, and retains that schema-3 object as `releaseCandidateBinding` inside the schema-2 result summary. The evidence directory must be new or empty. Prefer the process-scoped `LBB_TOKEN` environment input shown above: the runner consumes and clears it immediately, then passes the value to its children through explicit environment blocks. An optional `-Token` remains available for programmatic callers that already hold the value in memory, but placing a literal token in a new process command line or shell history is unsafe. The runner intentionally discards child-process stdout because the server prints its bearer token and filters unrelated window and tab collections plus raw text values from saved API responses. Each child starts suspended with `STARTUPINFOEX`; `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` restricts inheritance to the single intended NUL handle used for standard input, output, and error. The child enters a private Windows Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` before it resumes, so neither unrelated PowerShell handles nor helper worker descendants escape the runner's boundary. A failed pre-resume launch terminates the suspended child and verifies bounded process exit before releasing its only process handles; a cleanup failure is surfaced together with the original launch error. The runner's outer `finally` block stops an active share, requests fixture shutdown, terminates and verifies zero active Job-owned descendants, closes the kill-on-close handle, disposes the fixture/builder/self-test process handles, and then removes only the exact expected executable plus its now-empty build directory. Cleanup refuses a reparse point, an unexpected entry, or recursive deletion; any such condition invalidates the run instead of broadening the target. It then verifies that the selected loopback port is bindable again, scans every retained evidence file for the raw token and deletes any offending test-owned file while failing the run, removes the in-memory token variable, and writes `summary.json` on success or failure. Every summary records `candidateBinding.checksumManifestMatched: true`, the expected manifest hash, exact server/helper hashes, VERSIONINFO values, CLI-reported versions, and the sanitized `fixtureProcessBinding` without recording paths.

This harness is live Windows evidence, not a substitute for extension testing in real Chrome, clean-VM artifact verification, mixed-DPI coverage, UIPI negative tests, resize/minimize/close races, or long-duration endurance. `systemIndicator: true` is recorded only as the helper's no-suppression policy metadata; the exact-window screenshots remain non-proof. The `Capture` suite therefore compares separate pre-share and active-share desktop-level crops limited to the fixture target plus a 16-pixel band. A fixture-owned topmost `#101820` backdrop extends beyond that crop, and the runner deletes the crop unless at least 95% of its outer perimeter matches the backdrop. The saved provenance includes OS/session/capture metadata and SHA-256 proofs for the supplied server, helper, fixture, and runner, but intentionally omits artifact paths, hostname, username, and the rest of the desktop. The active-share border band must differ visibly from the baseline. Any missing arm acknowledgement, unstable pre-baseline sample, or later foreground invariant fails closed. Preserve that negative result; never convert an unbound retry into release evidence.

## Required checks

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
bash scripts/audit-versions.sh
bash scripts/check-licenses.sh
```

For documentation changes, also run:

```bash
git diff --check
rg -nP '\p{Hangul}' README.md SECURITY.md docs extension public src tests
```

The second command must return no matches because all repository and user-facing text is English.

## Extension package

Create the deterministic, allowlisted extension ZIP with:

```bash
bash scripts/package-extension.sh
```

The script rejects missing, linked, or unexpected package files and verifies that archive contents match the allowlisted extension sources plus the root project `LICENSE` byte for byte.

## Dependency licenses

`THIRD_PARTY_LICENSES.txt` is generated from the exact locked macOS and Windows production graphs with pinned `cargo-about 0.9.2`; build-only and test-only crates are excluded. Regenerate and verify it with:

```bash
cargo install cargo-about --locked --version 0.9.2 --features cli
bash scripts/check-licenses.sh --write
bash scripts/check-licenses.sh
```

The generator canonicalizes trailing whitespace and the final newline before writing. The gate rejects host paths, stale output, and the removed MPL-only dependency. Both distributed executables must print the checked-in report with `--licenses`; release-package verification also compares the archived notice files byte for byte.

## Evidence discipline

Plan verification before changing a capability. Keep these evidence classes separate:

1. Unit or contract proof: validates local logic and failure boundaries.
2. Integration proof: validates connector/server behavior.
3. Live application proof: validates the real browser or OS accepted the action.
4. Packaged release proof: validates the exact immutable artifacts users download.

Record negative results. A candidate run must not be described as published release evidence. Screenshots should show the relevant browser or OS indicator, target result, and non-interruption state without exposing tokens, personal data, or authenticated URLs.

The exact v0.12.9 packaged macOS attempt is a required negative-history reference: it stopped at the first semantic `setValue` after a cursor-position delta that the old record could not attribute. The exact [v0.12.10 attempt](../evidence/v0.12.10/computer/attempts/withdrawn-de59840-macos-deliberate-pointer-timeout/README.md) then passed 69 assertions before timing out with no separately authorized pointer movement and no final action. Version v0.12.11 was withdrawn before execution when its receipt could not authenticate both required fresh macOS lanes. The exact tagged v0.12.12 candidate was withdrawn after three deliberate-concurrency attempts stopped before product dispatch. Version v0.12.19 stopped before tagging after its second same-process PowerShell 7 relay self-test exposed a disposed task-backed async wait handle. The exact v0.12.20 quiet macOS lane passed, but its mandatory physical-pointer lane timed out before product dispatch; its Windows lane separately timed out because the action window was not discoverable under the state-mutating title. v0.12.21 was withdrawn before frozen candidate assembly or execution after review found that the macOS candidate binder could run `--version` and `--licenses` before the quiet-seat gate; its Windows CI lane also exposed a POSIX-only file-mode assertion under Node's Windows metadata projection. The exact v0.12.22 candidate passed the native quiet-seat gate and produced a Confirmed semantic `setValue` with every sealed route, focus, pointer, and Space invariant safe, but its harness incorrectly required independent keyboard-monitor fields that the sealed action schema does not contain and failed closed after 55 of 56 checks. Deliberate macOS, Windows, and Chrome never started; the protected publication run was canceled and no Release exists. Version v0.12.23 corrected that classifier mix-up without weakening either schema. Its quiet packaged lane passed 208/208 checks, and its deliberate lane accepted the exact app-share start receipt and completed 89/89 recorded assertions. It then reused a pre-handoff stream frame after 43.807 seconds; `computer.click` correctly failed HTTP 409 `COMPUTER_STALE_FRAME` before dispatch, so no completion receipt, Windows, Chrome, publication, or Release followed. Its exact ten-file negative record is preserved only on branch [`evidence/v0.12.23-macos-app-share-stale-frame-32746618027`](https://github.com/flrngel/local-browser-bridge/tree/4e4db75a4ede915d982d139a82dacac8a6c4772a/evidence/v0.12.23/computer/attempts/withdrawn-9e50811-macos-app-share-stale-frame). Do not rewrite or reuse those historical bytes. The v0.12.24 candidate reached the interactive Windows handoff, where its read-only watcher failed before operator action because exact system PowerShell 5.1 could not resolve `Read-AtomicRequestMarker` from the callback's closure-created dynamic module; Chrome and publication did not follow. Version v0.12.25 made the watcher portable and passed its packaged macOS lanes, but Windows stopped at `wait-foreground-arm` because the app-share did not expose the sentinel hosted by `powershell.exe`; stock-Chrome never started, the protected publication run was canceled, and no Release exists. Its exact negative Windows record is preserved on branch [`evidence/v0.12.25-windows-foreground-arm-timeout-32762398086`](https://github.com/flrngel/local-browser-bridge/tree/f480c57433bd6ddb7331b043d7c33d57822f5098/evidence/v0.12.25/computer/attempts/withdrawn-d193daf-windows-foreground-arm-timeout). A v0.12.26 success must come from fresh artifacts, use the source-bound dedicated Windows GUI fixture, retain the separated action/system classifiers, refresh strictly newer same-share/same-target/same-geometry action authority after the app-share receipt within the reserved deadline, and retain the corrected non-executing binder, explicit-argument portable marker checks, exact-app-share, and stable Windows-title contracts.

The v0.12.26 gate was not satisfied. Attempt 1 bound the frozen candidate and executed only the macOS quiet lane, which failed closed on externally observed shared-seat HID pointer activity during `computer.typeText`; its exact [negative evidence](https://github.com/flrngel/local-browser-bridge/tree/b1ba6a3bb77bc467e352716f099febb7c17fe767/evidence/v0.12.26/computer/attempts/withdrawn-0430e29-macos-quiet-pointer-contamination) was preserved before cancellation. Attempt 2 rebuilt the candidate but stopped before execution because the byte-identical extension returned one valid attestation for each workflow attempt and the verifier incorrectly required all returned attestations to name the current attempt; that [trust-gate record](https://github.com/flrngel/local-browser-bridge/tree/596e257d5d49845d7ca4f40e4f8282c99aba5687/evidence/v0.12.26/computer/attempts/withdrawn-0430e29-candidate-attestation-selection-mismatch) was also preserved before cancellation. Neither attempt reached Windows, stock Chrome, publication, or a public Release. Version 0.12.27 corrected exact-attempt attestation selection and added atomic verified five-file `dist/` replacement. It passed both packaged macOS lanes and independent audit; Windows trust, source, parser, and self-tests also passed. Its repository runner PowerShell process then exited before creating its evidence directory, while the ad-hoc external launcher retained no terminal streams, exit code, or process-start telemetry. Because the runner's packaged `--version` probes precede that directory, candidate-byte execution is outcome-unknown. No Computer Use action, Chrome run, evidence commit, approval, or Release followed, and the waiting publication job was canceled. Version 0.12.28 introduced the checked-in coordinator but remained a blocked source checkpoint. Version 0.12.29 completes its Windows-native source gate and must use fresh artifacts and evidence in every future candidate lane; none has run yet. The v0.12.45 candidate later passed both fresh macOS lanes and the corrected Windows release-candidate trust gate. Its one-shot Windows reservation was then consumed at the first packaged executable probe because the shipped GUI desktop host correctly reported `local-browser-bridge-desktop 0.12.45` while the native acceptance runner still required the obsolete server-binary string `local-browser-bridge 0.12.45`. No Computer Use action, screenshot, Chrome action, evidence commit, tag, or Release followed. Version 0.12.46 aligned that consumer with the already enforced Windows artifact contract and added a cross-file regression assertion. Its exact candidate was then withdrawn at the macOS receiver-probe timeout described above. Version 0.12.48 retains both repairs and requires fresh bytes.

See [SOTA audit](SOTA_AUDIT.md) and the [evidence index](../evidence/) for current boundaries.

The v0.12.47 candidate passed both repaired macOS lanes and mandatory visual
review, then was withdrawn when the coordinator supplied an empty aggregate
output path to the finalizer. Version 0.12.48 moves that dependent path
construction into the checked-in, self-tested finalizer wrapper.

## Versioning

Every completed extension/package work item or deployment must bump and align:

- the Rust package version;
- `Cargo.lock`;
- the extension manifest version; and
- the extension library version.

Run `bash scripts/audit-versions.sh` before packaging. Commit finished work with a Conventional Commits message unless the current task explicitly says not to commit.

## Deployment

In this repository, `deploy` means finishing the entire release operation, not
merely compiling or starting the bridge. The intended version must be committed
and pushed; one exact green `main` commit must become a frozen cross-platform
candidate; the required macOS, Windows, and stock-Chrome acceptance lanes must
pass against those bytes; and only then may those unchanged bytes become an
annotated tag and immutable public GitHub Release. Publication is a separate,
manual workflow so failed candidates do not create tags, Releases, or protected
environment deployments.

### Phase 1: build a tagless release candidate

The v0.12.42 candidate was withdrawn before candidate execution when the
bounded macOS acceptance extractor rejected the newly added desktop-host app
bundle that the release package correctly contained. Version 0.12.45 added that
bundle's exact seven-entry app layout to the acceptance inventory and exercised
the complete 17-entry archive in the package-preparer contract test. That
candidate passed the repaired trust gate but exposed the stale Windows native
`--version` expectation described above. Version 0.12.46 aligned native
acceptance with the shipped desktop host, then exposed the shorter macOS
receiver-watcher boundary. Version 0.12.47 covered that boundary and passed both
macOS lanes, then was withdrawn at the coordinator finalizer-path failure.
Version 0.12.48 retains the canonical inventory and Windows identity repair,
covers the complete server call boundary, and adds the finalizer wrapper.

Run `.github/workflows/deploy.yml` manually from `main`. Before dispatch, ensure
that the version is aligned across the Rust package and extension, the exact
source is the current `origin/main` tip, the required CI jobs are green for that
source tree, and neither `vVERSION` nor a Release with that name exists.

```bash
REPOSITORY="flrngel/local-browser-bridge"
VERSION="0.12.48"
SOURCE_SHA="EXACT_40_CHARACTER_GREEN_MAIN_SHA"

gh workflow run deploy.yml \
  --repo "$REPOSITORY" \
  --ref main \
  -f version="$VERSION" \
  -f source_sha="$SOURCE_SHA"
```

The candidate workflow has four jobs:

1. **Source and extension** binds the request to the exact green `main` source,
   runs the package/acceptance contract self-tests, packages the extension, and
   attests it.
2. **Windows** builds and verifies the x86_64 MSVC server and helper, then
   attests both executables.
3. **macOS** builds and verifies the universal server and stable helper app
   bundle, then attests the archive.
4. **Assemble** downloads those outputs, creates the canonical
   `SHA256SUMS.txt`, verifies the exact five-file set and exact-source
   attestations, and freezes one `release-candidate` artifact for 14 days.

This phase has no release environment and deliberately creates no tag, draft,
deployment, or Release. Record the workflow run ID, run attempt, artifact ID,
raw artifact-ZIP SHA-256, checksum-manifest SHA-256, source SHA, and all asset
hashes before executing candidate bytes. A rerun is a distinct attempt even
when its payload is byte-identical; never relabel evidence from one attempt as
evidence for another.

### Acceptance and the schema-3 receipt

Download and verify the exact frozen artifact, then run both fresh macOS lanes,
interactive Windows helper acceptance, and stock-Chrome acceptance against that
same set. The macOS gate must also verify both packaged-helper architecture
slices against the forbidden global-input API list and targeted dynamic-symbol
allowlist. This is shipped-route evidence, not Apple support for private APIs.

The accepted evidence commit must be a single candidate-bound commit whose sole
parent is the exact `main` source. It may add only the allowlisted, sanitized
evidence tree for this version, workflow run, and attempt. Keep the required
macOS lane results, Windows helper summary, stock-Chrome final record, referenced
screenshots, logs, operator markers, fixture records, sidecars, and independent
review records. Never retain credentials, bearer tokens, personal paths,
operator identity, raw browser/API data, or unrelated screen content.

For v0.12.48, the canonical evidence branch is
`evidence/v0.12.48-release-run-RUN_ID-attempt-RUN_ATTEMPT`, and its additions
live below
`evidence/v0.12.48/release/run-RUN_ID-attempt-RUN_ATTEMPT/`. The five primary
machine records are:

- `macos/macos-acceptance.json`;
- `macos/quiet/helper-results.json`;
- `macos/deliberate-concurrency/helper-results.json`;
- `windows/computer/summary.json`; and
- `windows/browser/browser-acceptance.json`.

After every required lane passes, create the canonical, nonsecret, one-line
schema-3 acceptance receipt using the checked-in evidence procedure. It binds
`refs/heads/main` and the exact source SHA to the candidate workflow run and
attempt, candidate artifact ID and raw ZIP digest, checksum manifest and asset
digests, and the exact evidence ref, commit, and result digests. There is no tag
object at this stage: the publication workflow creates the tag only after its
preflight succeeds. A missing, stale, differently ordered, malformed, or
cross-attempt receipt fails closed.

### Phase 2: publish the accepted bytes

Dispatch `.github/workflows/publish.yml` manually from `main`, passing the
receipt directly as a workflow input. The receipt is not a secret, but it must
not contain credentials or personal data.

```bash
REPOSITORY="flrngel/local-browser-bridge"
VERSION="0.12.48"
SOURCE_SHA="EXACT_40_CHARACTER_GREEN_MAIN_SHA"
CANDIDATE_RUN_ID="EXACT_GITHUB_RUN_ID"
CANDIDATE_RUN_ATTEMPT="EXACT_GITHUB_RUN_ATTEMPT"
ACCEPTANCE_RECEIPT_PATH="/absolute/path/to/acceptance-receipt.json"

gh workflow run publish.yml \
  --repo "$REPOSITORY" \
  --ref main \
  -f version="$VERSION" \
  -f source_sha="$SOURCE_SHA" \
  -f candidate_run_id="$CANDIDATE_RUN_ID" \
  -f candidate_run_attempt="$CANDIDATE_RUN_ATTEMPT" \
  -f acceptance_receipt="$(<"$ACCEPTANCE_RECEIPT_PATH")"
```

The first publication job runs outside the protected environment and has no
Release-mutation authority. It reconstructs the candidate from GitHub, raw-hashes
the artifact ZIP, verifies the exact five-file payload and attestations, fetches
and validates the remote evidence commit, and proves every schema-3 receipt and
evidence binding before preparing the immutable publication input. The receipt
is supplied per dispatch; there is no mutable release-environment receipt
variable.

Only the subsequent `release` job uses the protected `release` environment. It
must consume the exact preflight-approved bytes, recheck the repository and tag
policies, create annotated `vVERSION` for the accepted source, create the
immutable GitHub Release, re-download every published asset, and verify the
downloaded set byte for byte. If the workflow implements a temporary
publication-transfer artifact, cleanup may delete only that exact transfer
artifact. It must not delete the frozen candidate, evidence, unrelated workflow
artifacts, a tag, or a Release.

Repository policy remains part of the boundary. The tag ruleset must allow the
first creation of `refs/tags/v*` while forbidding later update or deletion with
no bypass actor. Third-party Actions remain pinned to full commit SHAs, and the
protected `release` environment requires its configured approval. Read mutable
repository settings back through the GitHub API during publication; source code
alone cannot prove them.

On a suitably configured macOS host, `scripts/deploy.sh` can produce a local
cross-platform candidate; official Windows artifacts require the MSVC target
through `cargo-xwin` or a native Windows build. Local outputs are useful for
early testing but are not release proof unless their exact SHA-256 values match
the frozen workflow candidate.

Verify a downloaded release set with:

```bash
bash scripts/verify-release-assets.sh VERSION dist
```

See [Installation](INSTALL.md) for the user-facing artifact verification flow
and [Architecture](ARCHITECTURE.md) for component boundaries.
