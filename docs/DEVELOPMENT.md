# Development

End users do not need Node.js. The entry points are Rust executables; the macOS build links a bundled Swift ScreenCaptureKit bridge and system frameworks. Developers running the complete contract suite need Node.js 24 for the checked-in extension and dashboard behavior harnesses, and some browser evidence rigs use the same test-only runtime. Node.js is never packaged into or invoked by the server, helper, or extension.

## Prerequisites

- Rust 1.88 or later with Cargo and rustfmt
- Node.js 24 for extension/dashboard behavior contracts and browser evidence rigs
- Platform SDK and linker for the target operating system
- `zip` and `unzip` for extension packaging
- Chrome or Edge 118+ for core browser testing
- Chrome or Edge 125+ for recursive cross-origin iframe testing
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
| `LBB_TOKEN_PATH` | Platform user-data path | Generated-token storage path; any non-default parent must already exist with private current-user-only permissions and is never rewritten |
| `LBB_DISABLE_UPDATE_CHECK` | `false` | Disable the one-time GitHub metadata check |

The equivalent `--no-update-check` flag disables the startup check; `--check-updates` performs the metadata-only check and exits. The server still binds only to loopback.

## Test the real extension

1. Open `chrome://extensions` or `edge://extensions`.
2. Enable Developer mode.
3. Select **Load unpacked** and choose `extension/`.
4. Enter the server token and port in the extension popup.
5. Use `/demo` for ordinary element, key, scroll, dialog, and navigation checks.
6. Confirm the Chrome-owned debugger warning, page pill, trusted Stop, revocation, and reconnect behavior.

Use Chromium 125 or later for the nested cross-origin fixtures. A passing same-process iframe test does not prove the OOPIF route.

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
- Input tests must prove an application-owned result plus unchanged platform-specific foreground/window-focus oracle, hardware pointer, and active desktop. The before/after samples do not prove the absence of an unobserved transient change.

Do not report cross-Space, minimized, protected, elevated, or framework-specific behavior as supported merely because a stream or message API returned success.

### Deterministic Windows live acceptance

`tests/fixtures/windows/WindowsComputerUseFixture.ps1` creates an animated target with a same-thread capture-evidence backdrop, a foreground sentinel on its own UI thread, and an optional magenta occluder on another UI thread. The target exposes a UI Automation `ValuePattern` edit and `InvokePattern` button, a retained-focus text edit, and a custom surface that records mouse, drag, wheel, `WM_KEY*`, and `WM_SYSKEY*` messages without recording character contents. Its state stores lengths and SHA-256 proofs instead of typed text.

Run it through `scripts/test-windows-computer-use.ps1` from a signed-in interactive Windows session. Use a dedicated test session: the fixture intentionally makes its own sentinel the foreground window during setup so subsequent background-control invariants have a deterministic oracle. The runner accepts only explicit server, helper, fixture, evidence-directory, and ephemeral-token inputs; it never installs software, changes security or network settings, dismisses warnings, or terminates processes it did not start. It refuses files carrying Windows download-zone metadata instead of unblocking them or bypassing a Windows warning.

```powershell
$server = (Resolve-Path .\dist\local-browser-bridge-vVERSION-windows-x86_64.exe).Path
$helper = (Resolve-Path .\dist\local-computer-helper-vVERSION-windows-x86_64.exe).Path
$fixture = (Resolve-Path .\tests\fixtures\windows\WindowsComputerUseFixture.ps1).Path
$evidence = Join-Path ([IO.Path]::GetTempPath()) ("lbb-windows-" + [Guid]::NewGuid().ToString("N"))
$bytes = New-Object byte[] 32
$random = [Security.Cryptography.RandomNumberGenerator]::Create()
$random.GetBytes($bytes)
$random.Dispose()
$token = [Convert]::ToBase64String($bytes).TrimEnd('=').Replace('+', '-').Replace('/', '_')
$env:LBB_TOKEN = $token

try {
  .\scripts\test-windows-computer-use.ps1 `
    -ServerPath $server `
    -HelperPath $helper `
    -FixturePath $fixture `
    -EvidenceDirectory $evidence `
    -Suite All `
    -ShowOccluder
} finally {
  Remove-Item Env:LBB_TOKEN -ErrorAction SilentlyContinue
  $token = $null
}
```

The independently runnable suites are `Smoke`, `Recovery`, `Semantic`, `Keyboard`, `Pixel`, `Capture`, and `Cancellation`; pass more than one as `-Suite Semantic,Pixel`, or use `All`. `Recovery` requires `-TimeoutSeconds 25` or greater; the default is 45. `Capture -ShowOccluder` verifies animated native-frame progression, exact-window occlusion exclusion, stream progression across an action, explicit stop, and the sanitized desktop-level indicator proof described below. `Cancellation` starts a real two-second target-routed move, waits for the fixture-owned `WM_MOUSEMOVE` counter instead of assuming dispatch from a timer, proves duplicate suppression, authenticated cancel, cached outcome-unknown replay, changed-request refusal, old-worker replacement, screenshot removal, late-frame quarantine, a replacement-session `NO_COMPUTER_FRAME` refusal because normalized coordinates have no observation dimensions before explicit observe, explicit one-shot observation recovery on the replacement worker, continued old-frame staleness, and a fresh recovered action. Unlike the macOS same-session proof, replacement attach clears the old session's authority gate; the Windows refusal proves missing replacement-session frame authority, not an inherited revocation gate. Every action suite saves exact-target screenshots plus app-owned result/state evidence and compares the foreground HWND, focused HWND, hardware cursor, input desktop, target activation count, and sentinel deactivation count before and after delivery.

`Recovery` is a launch-time-only fault proof, not a remotely callable protocol capability. The runner gives the helper supervisor a unique validated `Local\\LBBTestSharePump-*` manual-reset kernel-event name through `LBB_TEST_STALL_SHARE_PUMP_ONCE_EVENT`. The supervisor creates and holds the initially unsignaled event before launching a worker. The first disposable Windows worker signals it and stalls its first active-share conversion task; the worker's hard deadline emits a best-effort `COMPUTER_HELPER_WATCHDOG` error and exits, while the supervisor remains alive and replaces it. The suite requires the event to become signaled, a new direct-child worker PID and helper session ID, unchanged supervisor and server PIDs, continuously successful server-state polls, then a fresh exact-window observation and live native-share frame from the replacement worker. Replacement is accepted only when the server exposed `COMPUTER_HELPER_WATCHDOG` or at least 11.5 seconds elapsed after the runner observed the signaled fault event, providing a conservative timing lower bound for the 12-second watchdog; both share-start and event-relative durations are retained. Later workers observe the supervisor-held signaled event and do not stall. Closing the private Job releases the event; the runner verifies that it no longer exists. The hook neither creates a file nor adds a remotely triggerable protocol method.

The evidence directory must be new or empty. Prefer the process-scoped `LBB_TOKEN` environment input shown above: the runner consumes and clears it immediately, then passes the value to its children through explicit environment blocks. An optional `-Token` remains available for programmatic callers that already hold the value in memory, but placing a literal token in a new process command line or shell history is unsafe. The runner intentionally discards child-process stdout because the server prints its bearer token and filters unrelated window and tab collections plus raw text values from saved API responses. Each child starts suspended with `STARTUPINFOEX`; `PROC_THREAD_ATTRIBUTE_HANDLE_LIST` restricts inheritance to the single intended NUL handle used for standard input, output, and error. The child enters a private Windows Job Object configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` before it resumes, so neither unrelated PowerShell handles nor helper worker descendants escape the runner's boundary. A failed pre-resume launch terminates the suspended child and verifies bounded process exit before releasing its only process handles; a cleanup failure is surfaced together with the original launch error. The runner's outer `finally` block stops an active share, requests fixture shutdown, terminates and verifies zero active Job-owned descendants, closes the kill-on-close handle, verifies that the selected loopback port is bindable again, scans every retained evidence file for the raw token and deletes any offending test-owned file while failing the run, removes the in-memory token variable, and writes `summary.json` on success or failure.

This harness is live Windows evidence, not a substitute for extension testing in real Chrome, clean-VM artifact verification, mixed-DPI coverage, UIPI negative tests, resize/minimize/close races, or long-duration endurance. `systemIndicator: true` is recorded only as the helper's no-suppression policy metadata; the exact-window screenshots remain non-proof. The `Capture` suite therefore compares separate pre-share and active-share desktop-level crops limited to the fixture target plus a 16-pixel band. A fixture-owned topmost `#101820` backdrop extends beyond that crop, and the runner deletes the crop unless at least 95% of its outer perimeter matches the backdrop. The saved provenance includes OS/session/capture metadata and SHA-256 proofs for the supplied server, helper, fixture, and runner, but intentionally omits artifact paths, hostname, username, and the rest of the desktop. The active-share border band must differ visibly from the baseline. A failed foreground invariant can also mean the human used the session during the run; preserve that negative result and rerun only in a controlled interactive session.

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

The canonical release path is `.github/workflows/deploy.yml` from a matching annotated `vVERSION` tag. It is tag-push only; rerun the same tag-triggered workflow rather than dispatching it from a branch. Windows, macOS, and extension jobs build and attest their outputs; an assembly job then creates `SHA256SUMS.txt`, verifies the exact five-file set, and uploads one frozen `release-candidate` workflow artifact retained for 14 days. The publication job is bound to the protected, `v*`-only `release` environment and cannot start until its required reviewer approves it. Download that exact candidate from the waiting workflow run, record its source commit and asset SHA-256 values, and run the real macOS and FLRngel19 Windows/stock-Chrome acceptance suites against those bytes. Approve publication only after both pass. The gated job downloads the same workflow artifact, re-verifies every file and attestation, and publishes those unchanged bytes. Never substitute a local rebuild or post-publication smoke test for this pre-publication acceptance gate.

On a suitably configured macOS host, `scripts/deploy.sh` performs a local cross-platform candidate build; official Windows artifacts require the MSVC target through `cargo-xwin` or a native Windows build. Local outputs are useful for early testing but are not release proof unless their exact SHA-256 values match the frozen workflow candidate.

Verify a downloaded release set with:

```bash
bash scripts/verify-release-assets.sh VERSION dist
```

See [Installation](INSTALL.md) for the user-facing artifact verification flow and [Architecture](ARCHITECTURE.md) for component boundaries.
