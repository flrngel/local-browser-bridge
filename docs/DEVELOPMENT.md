# Development

End users do not need Node.js. The entry points are Rust executables; the macOS build links a bundled Swift ScreenCaptureKit bridge and system frameworks. Developers running the complete contract suite need Node.js 24 for the checked-in extension and dashboard behavior harnesses, and some browser evidence rigs use the same test-only runtime. Node.js is never packaged into or invoked by the server, helper, or extension.

## Prerequisites

- Rust 1.88 or later with Cargo and rustfmt
- Node.js 24 for extension/dashboard behavior contracts and browser evidence rigs
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

```bash
cargo build --locked --release
```

The two main binaries are:

```text
target/release/local-browser-bridge
target/release/local-computer-helper
```

The server embeds its UI. Do not add a runtime dependency on a JavaScript package manager, remote script, or CDN.

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

The v0.12.16 macOS harness defines two fresh, non-mergeable pointer-evidence lanes in [`evidence/v0.12.16/computer/README.md`](../evidence/v0.12.16/computer/README.md). The default `quiet` lane requires a healthy monitor, unchanged sampled position, no shared activity, and `sharedPointerActivityState: quiet` for every evidence cell. The separately authorized `deliberate-concurrency` lane requires at least one `contaminated` cell while the sealed helper route, helper-global-pointer preservation, target postcondition, and foreground/focus/Space boundaries still hold. Never convert or merge concurrency-lane bytes into quiet release evidence. An unknown monitor or boundary is a failure in both lanes.

Before either lane invokes a candidate binary—even with `--version`—the exact
tagged SystemProbe must complete a 30-second native quiet-seat epoch with at
least 60 stable transitions at 500 ms cadence. Any cumulative pointer/cursor,
foreground, AX focus/front-window, or active-Space change resets all epoch
progress under the original 30-minute deadline. Unknown or unhealthy monitoring
is immediately fatal. The runner records only bounded summary facts, starts the
fixture/server/helper after completion, and keeps every later per-action and
whole-run proof unchanged. The deliberate lane uses this gate before its
required initial quiet cells; intentional pointer movement begins only at the
separately verified orange prompt.

For the deliberate-concurrency lane, start the packaged evidence runner and then run `node scripts/wait-macos-pointer-concurrency-handoff.mjs --mode watch --evidence-dir "$ABSOLUTE_EVIDENCE_DIRECTORY" --runner-pid "$RUNNER_PID"` in a separate terminal. The watcher opens only the exact request and completion markers under the evidence directory's `operator` subdirectory, read-only and without following links. It requires schema 1, v0.12.16, fresh file-bound timestamps, the exact live runner/request/prompt identity, a visible nonactivating panel, notification-only/non-authoritative flags, and final `clickFreeMotionObserved: true` proof. `ACTION REQUIRED` means continuously move the shared pointer without clicking. Before product dispatch, button/drag/scroll/tablet activity or a foreground/focus/Space change resets the clean-motion arm under the same absolute deadline; an `ACTION` transition contaminated before dispatch returns to `MOVE`. Unknown monitoring, deadline-independent probe failure, or contamination once dispatch is in flight still fails closed. Stop only after `COMPLETE`, which requires click-free movement to advance through the independently observed green completion state and both product and independent boundaries to report sustained contamination. Entering `ACTION` starts a separate 10-second hard action/completion grace, capped by a 310-second total prompt lifetime, and the panel exits at the earlier deadline in every state. The watcher never writes an acknowledgement and its output is not acceptance authority. A stale or changed marker, dead runner or prompt, mismatched completion, unknown boundary, or timeout fails closed. Validate the observer and producer classifier without UI or evidence files with `node --check scripts/wait-macos-pointer-concurrency-handoff.mjs`, `node scripts/wait-macos-pointer-concurrency-handoff.mjs --mode self-test`, `node --check evidence/v0.12.16/computer/helper-evidence-rig.mjs`, and `node evidence/v0.12.16/computer/helper-evidence-rig.mjs --self-test`.

### Deterministic Windows live acceptance

`tests/fixtures/windows/WindowsComputerUseFixture.ps1` creates an animated target with a same-thread capture-evidence backdrop, a foreground sentinel on its own UI thread, and an optional magenta occluder on another UI thread. The target exposes a UI Automation `ValuePattern` edit and `InvokePattern` button, a retained-focus text edit, and a custom surface that records mouse, drag, wheel, `WM_KEY*`, and `WM_SYSKEY*` messages without recording character contents. Its state stores lengths and SHA-256 proofs instead of typed text.

Run it through `scripts/test-windows-computer-use.ps1` from a signed-in interactive Windows session. Use a dedicated test session. After the helper, recovery event, and exact target binding are ready, the runner posts a fresh test-only generation to the fixture. It waits for a separate bounded receipt proving that exact generation was processed once, the exact same-thread fixture-owned root/button topology still holds, the orange sentinel has enabled its large **CLICK TO ARM** button, and input is either untouched or already one internally consistent acknowledgement. This second form handles a valid click made as soon as the button visibly enables. The sentinel is initially shown without activation; the deliberate click can still activate and focus it normally. The runner saves the delivery proof, atomically creates the runner-create-once notification `operator/foreground-arm-request.json`, and prints `ACTION REQUIRED`: click only if the button still says **CLICK TO ARM**; if it already says **ARMED**, do not click again. A human or separately trusted Computer Use surface must left-click that button once and stop using the session. The fixture accepts the mouse down only while native global foreground equals the sentinel and the calling UI thread's focus equals the exact button; focus loss, deactivation, or a new generation clears the pending press. Mouse up must repeat the exact generation, button, point, foreground, and focus checks before acknowledgement. Total button left-down/up attempt counts must each remain exactly one. After the click and three stable native samples, the runner atomically creates the matching `operator/foreground-arm-received.json`; it still requires an exact arm-to-baseline continuity check before any product action. Both operator files are sanitized, create-once notifications. The runner never reads them and they are never acceptance authority, so a copied, stale, changed, or replayed marker cannot satisfy the click proof. Before the first baseline command, it re-binds the original authenticated helper session and PID to the same exact-image direct child and completes another `computer.status` round trip; a worker restart during the arm wait fails closed. It never calls `SetForegroundWindow`, synthesizes an Alt key, or loops to steal focus. This is a trusted interactive-session receipt, not cryptographic proof that a physical human produced the window messages; the acceptance session must exclude untrusted same-integrity injectors. The explicit setup boundary makes subsequent background-control invariants deterministic without treating `TopMost`, `Shown`, or a WinForms activation callback as global-foreground proof. The runner requires the intended version, the frozen candidate's exact `SHA256SUMS.txt`, its SHA-256 recorded independently by the release coordinator, canonical server/helper filenames, fixture, evidence directory, and ephemeral token. Before launch it requires the manifest's exact four-asset inventory, matches both executable hashes, checks both VERSIONINFO versions, and checks bounded `--version` output. It never installs software, changes security or network settings, dismisses warnings, or terminates processes it did not start. It refuses files carrying Windows download-zone metadata instead of unblocking them or bypassing a Windows warning.

Live mode intentionally runs only under the system Windows PowerShell 5.1 Desktop host because the acceptance fixture is WinForms/.NET Framework code. PowerShell 7 remains a parser and runner-self-test surface; the runner refuses it before creating any child process or evidence directory.

```powershell
$server = (Resolve-Path .\dist\local-browser-bridge-vVERSION-windows-x86_64.exe).Path
$helper = (Resolve-Path .\dist\local-computer-helper-vVERSION-windows-x86_64.exe).Path
$manifest = (Resolve-Path .\dist\SHA256SUMS.txt).Path
$candidateBinding = (Resolve-Path .\candidate-binding.json).Path
# Copy this value from the coordinator's independently recorded frozen-candidate inventory.
$manifestSha256 = "EXPECTED_64_CHARACTER_SHA256"
$fixture = (Resolve-Path .\tests\fixtures\windows\WindowsComputerUseFixture.ps1).Path
$evidence = Join-Path ([IO.Path]::GetTempPath()) ("lbb-windows-" + [Guid]::NewGuid().ToString("N"))
$bytes = New-Object byte[] 32
$random = [Security.Cryptography.RandomNumberGenerator]::Create()
$random.GetBytes($bytes)
$random.Dispose()
$token = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
$env:LBB_TOKEN = $token

try {
  $windowsPowerShell = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
  & $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File `
    .\scripts\test-windows-computer-use.ps1 `
    -Version VERSION `
    -ServerPath $server `
    -HelperPath $helper `
    -ChecksumManifest $manifest `
    -ChecksumManifestSha256 $manifestSha256 `
    -CandidateBindingPath $candidateBinding `
    -FixturePath $fixture `
    -EvidenceDirectory $evidence `
    -ForegroundArmTimeoutSeconds 300 `
    -Suite All `
    -ShowOccluder
  if ($LASTEXITCODE -ne 0) { throw "Windows computer-use acceptance failed." }
} finally {
  Remove-Item Env:LBB_TOKEN -ErrorAction SilentlyContinue
  $token = $null
}
```

For a remote coordinator, do not block on one opaque child invocation. Reserve
the external Windows UI surface before starting the runner: prefer an authorized
Windows Computer Use app-share, and retain a human on the Windows session as the
fallback. Start the runner in a retained terminal session with a short initial
yield and record its process ID plus exact UTC process start time. Then run the
repository's read-only watcher from a separate process:

```powershell
& .\scripts\wait-windows-foreground-arm-handoff.ps1 `
  -Mode Watch `
  -EvidenceDirectory $evidence `
  -RunnerProcessId $runnerPid `
  -RunnerStartedAtUtc $runnerStartedAtUtc `
  -WaitTimeoutSeconds 300
```

The watcher reads only `operator/foreground-arm-request.json`. It requires the
exact v0.12.16/schema-2 field set and order, a fresh non-expired publication,
ordinary non-reparse paths, and the same live runner PID/start time both before
and after parsing. It emits exactly one compact sanitized
`foreground-arm-visual-handoff` JSON object or fails closed. It neither writes
evidence nor reads product state, moves focus, injects input, or verifies the
separate external authorization.

Treat that output as routing, not consent or authority. Obtain a fresh one-shot
authorization for the named external surface, observe a fresh shared frame, and
left-click exactly once only if it visibly shows the exact orange **LBB Windows
Acceptance - ACTION REQUIRED** window and **CLICK TO ARM** button. If the frame
shows **ARMED**, is stale, or is ambiguous, perform zero clicks. Do not make a
preparatory focus click, use a keyboard/UIA/scripted substitute, or retry an
unknown outcome. Stop all UI interaction immediately after the one action and
poll only runner liveness, the matching received notification, and final
`summary.json`. Neither notification can satisfy acceptance; only the fixture's
exact click/native proof and final summary/inventory can pass. Keep watcher and
runner stdout/stderr outside the evidence directory and delete those
coordinator-owned files after sanitized review.

Exercise the watcher parser, liveness/freshness refusals, action-required
one-click handoff, and already-armed zero-click handoff without opening UI:

```powershell
powershell.exe -NoLogo -NoProfile -File `
  .\scripts\wait-windows-foreground-arm-handoff.ps1 -Mode SelfTest
```

The expected sole output is `Windows foreground-arm handoff watcher self-test passed.`

The fixture increments a monotonic publication generation on every state write. Arm stability consumes three distinct advancing publications after the delivery receipt, the baseline consumes another, and every later invariant comparison requires a still-newer publication. Replaying one valid state file or stalling the fixture writer therefore cannot satisfy the acceptance oracle.

The independently runnable suites are `Smoke`, `Recovery`, `Semantic`, `Keyboard`, `Pixel`, `Capture`, and `Cancellation`; pass more than one as `-Suite Semantic,Pixel`, or use `All`. `Recovery` requires `-TimeoutSeconds 25` or greater; the default is 45. Foreground arming has its own monotonic 15–300 second bound and defaults to a bounded 300-second runner arm interval; product request-delivery and command timeouts are still separate and do not expand. The separate request-delivery phase is bounded to ten seconds. Sanitized proofs record request/ack generation equality, exact button identity, native same-thread owner/root/child topology, request and acknowledgement counts, exactly-one left-down/up attempt counts, button-enabled state, stable-sample count, arm-to-baseline continuity, and timeout—never raw window handles or cursor coordinates. A successful `-Suite All -ShowOccluder` run retains exactly 88 files: three fixture records, 62 step records, 20 sanitized screenshots, two operator notifications, and `summary.json`; a pre-click timeout retains only the request marker and the files reached before failure. `Capture -ShowOccluder` verifies animated native-frame progression, exact-window occlusion exclusion, stream progression across an action, explicit stop, and the sanitized desktop-level indicator proof described below. `Cancellation` starts a real two-second target-routed move, waits for the fixture-owned `WM_MOUSEMOVE` counter instead of assuming dispatch from a timer, proves duplicate suppression, authenticated cancel, cached outcome-unknown replay, changed-request refusal, old-worker replacement, screenshot removal, late-frame quarantine, a replacement-session `NO_COMPUTER_FRAME` refusal because normalized coordinates have no observation dimensions before explicit observe, explicit one-shot observation recovery on the replacement worker, continued old-frame staleness, and a fresh recovered action. Unlike the macOS same-session proof, replacement attach clears the old session's authority gate; the Windows refusal proves missing replacement-session frame authority, not an inherited revocation gate. Every action suite saves exact-target screenshots plus app-owned result/state evidence and compares the foreground HWND, focused HWND, OS-global cursor position, input desktop, target activation count, sentinel deactivation count, arm generations, arm request/ack counts, and arm-button input-attempt counts before and after delivery.

`Recovery` is a launch-time-only fault proof, not a remotely callable protocol capability. The runner gives the helper supervisor a unique validated `Local\\LBBTestSharePump-*` manual-reset kernel-event name through `LBB_TEST_STALL_SHARE_PUMP_ONCE_EVENT`. The supervisor creates and holds the initially unsignaled event before launching a worker. The first disposable Windows worker signals it and stalls its first active-share conversion task; the worker's hard deadline emits a best-effort `COMPUTER_HELPER_WATCHDOG` error and exits, while the supervisor remains alive and replaces it. The suite requires the event to become signaled, a new direct-child worker PID and helper session ID, unchanged supervisor and server PIDs, continuously successful server-state polls, then a fresh exact-window observation and live native-share frame from the replacement worker. Replacement is accepted only when the server exposed `COMPUTER_HELPER_WATCHDOG` or at least 11.5 seconds elapsed after the runner observed the signaled fault event, providing a conservative timing lower bound for the 12-second watchdog; both share-start and event-relative durations are retained. Later workers observe the supervisor-held signaled event and do not stall. Closing the private Job releases the event; the runner verifies that it no longer exists. The hook neither creates a file nor adds a remotely triggerable protocol method.

The manifest SHA-256 is an out-of-band binding value: do not derive it from the same untrusted copy immediately before the run. The release coordinator records it when downloading the exact gated workflow artifact and sends that value with the candidate. The runner also requires the trust wrapper's create-once `candidate-binding.json`, validates its exact source/tag/run/attempt/artifact/raw-ZIP/attestation and five-file facts, and retains those sanitized facts as `releaseCandidateBinding` in schema-2 `summary.json`. The evidence directory must be new or empty. Prefer the process-scoped `LBB_TOKEN` environment input shown above: the runner consumes and clears it immediately, then passes the value to its children through explicit environment blocks. An optional `-Token` remains available for programmatic callers that already hold the value in memory, but placing a literal token in a new process command line or shell history is unsafe. The runner intentionally discards child-process stdout because the server prints its bearer token and filters unrelated window and tab collections plus raw text values from saved API responses. Each child starts suspended with `STARTUPINFOEX`; `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` restricts inheritance to the single intended NUL handle used for standard input, output, and error. The child enters a private Windows Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` before it resumes, so neither unrelated PowerShell handles nor helper worker descendants escape the runner's boundary. A failed pre-resume launch terminates the suspended child and verifies bounded process exit before releasing its only process handles; a cleanup failure is surfaced together with the original launch error. The runner's outer `finally` block stops an active share, requests fixture shutdown, terminates and verifies zero active Job-owned descendants, closes the kill-on-close handle, verifies that the selected loopback port is bindable again, scans every retained evidence file for the raw token and deletes any offending test-owned file while failing the run, removes the in-memory token variable, and writes `summary.json` on success or failure. Every summary records `candidateBinding.checksumManifestMatched: true`, the expected manifest hash, exact server/helper hashes, VERSIONINFO values, and CLI-reported versions without recording paths.

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

The exact v0.12.9 packaged macOS attempt is a required negative-history reference: it stopped at the first semantic `setValue` after a cursor-position delta that the old record could not attribute. The exact [v0.12.10 attempt](../evidence/v0.12.10/computer/attempts/withdrawn-de59840-macos-deliberate-pointer-timeout/README.md) then passed 69 assertions before timing out with no separately authorized pointer movement and no final action. Version v0.12.11 was withdrawn before execution when its receipt could not authenticate both required fresh macOS lanes. The exact tagged v0.12.12 candidate was withdrawn after three deliberate-concurrency attempts stopped before product dispatch: a final probe inherited the nearly exhausted arm budget, a pre-arm pointer interval contained disallowed input, and an `ACTION` transition contained input plus user-context contamination. The three records remain isolated on their evidence-only branches. Windows/stock-Chrome never completed for those candidates. Do not rewrite those historical failures with v0.12.16 field names. A v0.12.16 success must come from fresh artifacts and the new pre-dispatch re-arm plus route/activity/postcondition contract.

See [SOTA audit](SOTA_AUDIT.md) and the [evidence index](../evidence/) for current boundaries.

## Versioning

Every completed extension/package work item or deployment must bump and align:

- the Rust package version;
- `Cargo.lock`;
- the extension manifest version; and
- the extension library version.

Run `bash scripts/audit-versions.sh` before packaging. Commit finished work with a Conventional Commits message unless the current task explicitly says not to commit.

## Deployment

In this repository, `deploy` means more than a local build. It means committing and pushing the intended version, building the Windows server and helper, building the universal macOS server/helper archive, packaging the matching extension, publishing all artifacts plus `SHA256SUMS.txt` and GitHub provenance in an immutable public GitHub Release, then downloading and verifying every published asset.

The canonical release path is `.github/workflows/deploy.yml` from a matching annotated `vVERSION` tag. It is tag-push only; rerun that tag-triggered workflow rather than dispatching it from a branch, but treat every rerun as a new attempt with a new artifact and receipt. Automatic draft recovery is deliberately limited to a byte-identical draft bound to the current attempt's exact receipt. A draft left by an older failed attempt cannot be relabelled or automatically deleted by the new attempt; inspect and remove that unpublished draft through an explicitly authorized recovery operation before starting fresh acceptance. Windows, macOS, and extension jobs build and attest their outputs; an assembly job then creates `SHA256SUMS.txt`, verifies the exact five-file set, and uploads one frozen `release-candidate` workflow artifact retained for 14 days. The publication job is bound to the protected, `v*`-only `release` environment and cannot start until its required reviewer approves it. Download that exact candidate from the waiting workflow run, record its source commit and asset SHA-256 values, and run the real macOS and designated Windows/stock-Chrome acceptance suites against those bytes. The macOS gate must also verify each architecture slice of the exact packaged helper against the forbidden global-input API list and expected targeted dynamic-symbol allowlist; this is shipped-route evidence, not Apple support for private SkyLight. Approve publication only after every required lane passes. The gated job downloads the same workflow artifact, re-verifies every file and attestation, and publishes those unchanged bytes. Never substitute a local rebuild or post-publication smoke test for this pre-publication acceptance gate.

Publication also requires one candidate-bound evidence commit and a nonsecret
canonical V2 receipt. The evidence branch is exactly
`evidence/v0.12.16-release-run-RUN_ID-attempt-RUN_ATTEMPT`; its tip must be one
commit whose sole parent is the tagged source commit. That commit may only add
ordinary mode-`100644` allowlisted files beneath
`evidence/v0.12.16/release/run-RUN_ID-attempt-RUN_ATTEMPT/`. The five required
machine records are:

- `macos/macos-acceptance.json`;
- `macos/quiet/helper-results.json`;
- `macos/deliberate-concurrency/helper-results.json`;
- `windows/computer/summary.json`; and
- `windows/browser/browser-acceptance.json`.

The verifier also requires the result-referenced lane screenshots, logs,
operator markers, Windows steps/fixture records, and the browser finalizer's
exact schema-V3 inventory of 21 input files and 22 finalized files. It validates
duplicate-free JSON/NDJSON, PNG structure and metadata policy, cleanup,
candidate-bound approval, digest-bound independent exported-file review, the
preflight/postflight external-surface attestations, candidate identity, and a
leakage denylist. Never copy credentials, tokens, paths, operator identity, raw
browser/API data, or unrelated screen content into the evidence commit.

Only after both fresh macOS lanes, interactive Windows helper acceptance, and
stock-Chrome acceptance pass against the same frozen artifact should the
coordinator create the exact one-line receipt. Its keys and their order are
canonical; run and artifact identifiers are strings:

```bash
receipt="$(jq -cn \
  --arg tag "v0.12.16" \
  --arg source_sha "EXACT_VERIFIED_SOURCE_SHA" \
  --arg tag_object_sha "EXACT_ANNOTATED_TAG_OBJECT_SHA" \
  --arg run_id "EXACT_GITHUB_RUN_ID" \
  --arg run_attempt "EXACT_GITHUB_RUN_ATTEMPT" \
  --arg artifact_id "EXACT_RELEASE_CANDIDATE_ARTIFACT_ID" \
  --arg artifact_zip_sha256 "EXACT_RAW_ARTIFACT_ZIP_SHA256" \
  --arg manifest_sha256 "$(shasum -a 256 SHA256SUMS.txt | awk '{ print $1 }')" \
  --arg evidence_ref "refs/heads/evidence/v0.12.16-release-run-EXACT_GITHUB_RUN_ID-attempt-EXACT_GITHUB_RUN_ATTEMPT" \
  --arg evidence_commit_sha "EXACT_REMOTE_EVIDENCE_COMMIT_SHA" \
  --arg macos_acceptance_sha256 "$(shasum -a 256 macos/macos-acceptance.json | awk '{ print $1 }')" \
  --arg macos_quiet_sha256 "$(shasum -a 256 macos/quiet/helper-results.json | awk '{ print $1 }')" \
  --arg macos_deliberate_sha256 "$(shasum -a 256 macos/deliberate-concurrency/helper-results.json | awk '{ print $1 }')" \
  --arg windows_sha256 "$(shasum -a 256 windows/computer/summary.json | awk '{ print $1 }')" \
  --arg chrome_sha256 "$(shasum -a 256 windows/browser/browser-acceptance.json | awk '{ print $1 }')" \
  '{schemaVersion:2,tag:$tag,sourceSha:$source_sha,tagObjectSha:$tag_object_sha,workflowRunId:$run_id,workflowRunAttempt:$run_attempt,releaseCandidateArtifactId:$artifact_id,releaseCandidateArtifactZipSha256:$artifact_zip_sha256,checksumManifestSha256:$manifest_sha256,evidenceRef:$evidence_ref,evidenceCommitSha:$evidence_commit_sha,macosPassed:true,macosAcceptanceSha256:$macos_acceptance_sha256,macosQuietResultSha256:$macos_quiet_sha256,macosDeliberateConcurrencyResultSha256:$macos_deliberate_sha256,windowsPassed:true,windowsResultSha256:$windows_sha256,stockChromePassed:true,stockChrome:true,stockChromeResultSha256:$chrome_sha256}')"
```

Set that exact value as `LBB_RELEASE_ACCEPTANCE_V2` on the protected GitHub
`release` environment, then approve the waiting job. The variable and evidence
branch are selectors, not authority. Before any Release mutation, the checked-in
verifier independently binds the current workflow attempt and assembly job,
queries the sole live `release-candidate` artifact, downloads and raw-hashes
its ZIP, compares its exact payload with the job download, fetches the remote
evidence commit, proves its sole parent/tree, and validates every result,
sidecar, digest, and binding. The accepted receipt SHA-256 is embedded in the
immutable release marker and visible notes. A rerun has a new attempt, artifact,
evidence ref/commit, and receipt; evidence from another attempt is never
relabelled or reused.

Repository policy is part of that boundary. An active tag ruleset allows the first creation of `refs/tags/v*` but forbids every later update or deletion with no bypass actor. GitHub Actions requires every third-party action reference to use a complete commit SHA. Dependabot security updates, secret-scanning push protection, and private vulnerability reporting remain enabled. Before tagging, read these settings back through the GitHub API; a source contract cannot prove mutable repository policy.

On a suitably configured macOS host, `scripts/deploy.sh` performs a local cross-platform candidate build; official Windows artifacts require the MSVC target through `cargo-xwin` or a native Windows build. Local outputs are useful for early testing but are not release proof unless their exact SHA-256 values match the frozen workflow candidate.

Verify a downloaded release set with:

```bash
bash scripts/verify-release-assets.sh VERSION dist
```

See [Installation](INSTALL.md) for the user-facing artifact verification flow and [Architecture](ARCHITECTURE.md) for component boundaries.
