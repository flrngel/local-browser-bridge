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

### CI-hosted acceptance

Release acceptance no longer depends on an operator machine. The same driver
runs on GitHub-hosted `windows-latest` and `macos-26` runners and on a
developer machine:

```bash
cargo build --locked --release --bins
node scripts/ci-acceptance.mjs --mode source --dir target/release --out /tmp/lbb-acceptance \
  --chrome "/path/to/Chrome for Testing"
```

`scripts/ci-acceptance.mjs` (Node 24, no npm dependencies) starts the real
server binary with `LBB_TOKEN`/`LBB_PORT`/`LBB_ENABLE_SHELL`, checks
`/health`, `/api/state`, the dashboard, and the version; exercises `shell.run`
over POST and the Agent Fetch GET surface plus the `401` and `SHELL_DISABLED`
refusals; launches a pinned Chrome for Testing build with the unpacked
extension, bootstraps the token through `chrome.storage.local`, and drives
`tabs.new`, `browser.control.start`, `page.observe`, `page.click`,
`page.fill`, `page.evaluate`, `page.navigate`, `page.waitFor`, and
`browser.control.stop` against a local fixture page; then builds the native
fixture (`tests/fixtures/macos/CiAcceptanceFixture.swift` or
`tests/fixtures/windows/WindowsComputerUseFixture.ps1`), runs the helper's
`--request-permissions` probe, and exercises `computer.status`, window
selection, `computer.observe` with a saved screenshot, `computer.share.*`,
`computer.invoke`, `computer.setValue`, `computer.click`, and
`computer.typeText` with fixture-owned postconditions. Every check is recorded
as `{name, lane, status: pass|fail|skip, required, reason, evidence}` in
`<out>/acceptance.json` next to the screenshots and redacted logs.

Server, shell, and browser checks are required on both platforms. The computer
lane is required on Windows. On macOS it is required unless the permission
probe reports that Screen Recording, Accessibility, or input routing is
unavailable; in that case the positive checks are recorded as `skip` with
reason `permission-unavailable` and the documented refusals become the
required checks instead: `COMPUTER_CAPTURE_FAILED` for capture,
`semanticAvailable: false` for Accessibility, and `COMPUTER_INPUT_FAILED`
for input routing. `--lanes` restricts a local run, for example
`--lanes server,shell`.

`.github/workflows/acceptance.yml` is the reusable workflow around that
driver. CI calls it in `source` mode on every pull request;
`deploy.yml` calls it in `artifact` mode against the frozen candidate of the
same run (see [Deployment](#deployment)).

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

The v0.12.26 gate was not satisfied. Attempt 1 bound the frozen candidate and executed only the macOS quiet lane, which failed closed on externally observed shared-seat HID pointer activity during `computer.typeText`; its exact [negative evidence](https://github.com/flrngel/local-browser-bridge/tree/b1ba6a3bb77bc467e352716f099febb7c17fe767/evidence/v0.12.26/computer/attempts/withdrawn-0430e29-macos-quiet-pointer-contamination) was preserved before cancellation. Attempt 2 rebuilt the candidate but stopped before execution because the byte-identical extension returned one valid attestation for each workflow attempt and the verifier incorrectly required all returned attestations to name the current attempt; that [trust-gate record](https://github.com/flrngel/local-browser-bridge/tree/596e257d5d49845d7ca4f40e4f8282c99aba5687/evidence/v0.12.26/computer/attempts/withdrawn-0430e29-candidate-attestation-selection-mismatch) was also preserved before cancellation. Neither attempt reached Windows, stock Chrome, publication, or a public Release. Version 0.12.27 corrected exact-attempt attestation selection and added atomic verified five-file `dist/` replacement. It passed both packaged macOS lanes and independent audit; Windows trust, source, parser, and self-tests also passed. Its repository runner PowerShell process then exited before creating its evidence directory, while the ad-hoc external launcher retained no terminal streams, exit code, or process-start telemetry. Because the runner's packaged `--version` probes precede that directory, candidate-byte execution is outcome-unknown. No Computer Use action, Chrome run, evidence commit, approval, or Release followed, and the waiting publication job was canceled. Version 0.12.28 introduced the checked-in coordinator but remained a blocked source checkpoint. Version 0.12.29 completes its Windows-native source gate and must use fresh artifacts and evidence in every future candidate lane; none has run yet. The v0.12.45 candidate later passed both fresh macOS lanes and the corrected Windows release-candidate trust gate. Its one-shot Windows reservation was then consumed at the first packaged executable probe because the shipped GUI desktop host correctly reported `local-browser-bridge-desktop 0.12.45` while the native acceptance runner still required the obsolete server-binary string `local-browser-bridge 0.12.45`. No Computer Use action, screenshot, Chrome action, evidence commit, tag, or Release followed. Version 0.12.46 aligned that consumer with the already enforced Windows artifact contract and added a cross-file regression assertion. Its exact candidate was then withdrawn at the macOS receiver-probe timeout described above. Version 0.12.49 retained both repairs and passed both fresh macOS lanes, but its one-shot Windows run failed before UI use when the process-bound readiness probe returned zero exact-image children for an authenticated disposable worker. No screenshot, Computer Use action, Chrome action, tag, or Release followed. Version 0.12.50 removed the redundant Toolhelp basename prefilter while retaining direct-parent, exact live full-image-path, protocol-session, worker-PID, and interactive-session binding. Its quiet macOS lane and visual review passed, but its deliberate runner terminated before candidate execution on a coordinator scratch-path error. Version 0.12.59 bound the authenticated controller and worker independently, then its exact Windows attempt exposed a valid live-image path alias that the runner rejected by string spelling. Version 0.12.60 binds that live worker through volume/file identity. Its exact candidate passed both macOS lanes, Windows trust, helper readiness, and the single foreground-arm action, then failed at the first observation when a prematurely truncated QPC conversion classified the WGC compositor timestamp outside the monotonic range. Version 0.12.61 repaired the precision loss, but its fresh Windows attempt showed that the compositor timestamp can still lead a later user-mode QPC sample; it failed at the first observation after the single foreground-arm action and did not run stock Chrome. Version 0.12.62 saturates that future lead to zero elapsed age. Its macOS lanes passed, but the sole Windows attempt retained only a `reserved-no-retry` record and no conclusive terminal state, so it is `candidate-execution-unknown` and cannot be retried. Version 0.12.67 carries the fix with entirely fresh bytes and evidence.

The terminal v0.12.67 Windows attempt used source
`4d47485499fa855fdfe788bf3a9317979727d1f4`, candidate workflow run
`33401392061` attempt 1, and artifact `9761807586`. Candidate trust and
protocol-bound helper readiness passed. The runner published the foreground-arm
request but observed zero fixture left-mouse-down events, zero left-mouse-up
events, and zero acknowledgements. It exhausted the full 300-second
foreground-arm deadline before the baseline, any product action, or stock-Chrome
acceptance. Its persistent reservation is terminal `reserved-no-retry` with
`retryAllowed: false`. The exact sanitized negative evidence is immutable on
branch `evidence/v0.12.67-windows-foreground-arm-timeout-33401392061-attempt-1`
at commit
[`50070fdda84b329a5bcd9f6a5a7fceadf36add3c`](https://github.com/flrngel/local-browser-bridge/tree/50070fdda84b329a5bcd9f6a5a7fceadf36add3c/evidence/v0.12.67/computer/attempts/withdrawn-98fcda7-windows-foreground-arm-timeout).
Version 0.12.68 replaces that operator-dependent click gate with the automatic
stable external-foreground proof described above; no v0.12.67 byte,
reservation, marker, or result is reusable.

The exact v0.12.65 candidate was later withdrawn during its sole Windows
computer acceptance attempt. The recovery suite enqueued
`computer.share.start`, the helper connection ended before its outcome could be
established, and the coordinator recorded terminal, non-retriable
`COMMAND_OUTCOME_UNKNOWN`. The sanitized
[negative record](https://github.com/flrngel/local-browser-bridge/tree/bf8dbe595e7c7beb9a0817f0262fde46c1e49578/evidence/v0.12.65/computer/attempts/withdrawn-9a18c44-windows-share-start-outcome-unknown)
is immutable. Stock-Chrome acceptance, tagging, and publication did not follow.
Version 0.12.68 uses fresh source, candidate bytes, reservation, and evidence;
none of the v0.12.65 candidate lineage is reusable.

The sole exact v0.12.66 candidate was built from
`2dcc8eccebaed6837a3eb71012e4826d7e9de922`. Both packaged macOS lanes passed,
but its one-shot Windows reservation was consumed when the recovery suite's
initial `computer.share.start` exceeded the helper's 12-second command
watchdog. No named injected-fault event or replacement-worker causality was
retained, so that run does not prove that the injected failure occurred. A
later prelaunch correctly refused the existing reservation. Stock-Chrome
acceptance, evidence publication, tagging, and a public Release did not follow.
Version 0.12.68 bounds WGC startup and startup rollback below the outer helper
watchdog, observes the exact injected-fault event before classifying the
asynchronous response, and classifies `COMPUTER_HELPER_WATCHDOG` as an
unavailable connector that requires reconnection. It uses entirely fresh
source, candidate bytes, reservations, and evidence; no v0.12.66 byte or result
is reusable.

The exact v0.12.58 candidate later passed both macOS lanes and Windows trust,
then its native runner saw an authenticated, session-matched helper worker for
265 polls while Toolhelp returned zero exact-image direct children. It failed
at `bind-initial-helper-readiness`; no sentinel, Computer Use click, Chrome
action, tag, or Release followed. Version 0.12.59 bound the authenticated
`controllerProcessId` to the exact launched supervisor and the reported worker
to a queried image path and interactive session. Its exact candidate passed
both macOS lanes and Windows trust, then failed before UI use because that
valid image path did not string-equal the candidate spelling. The sanitized
[negative record](../evidence/v0.12.59/computer/attempts/withdrawn-ece060c-windows-helper-readiness-image-mismatch/README.md)
contains no absolute path. Version 0.12.60 compares the live image and candidate
by volume serial plus file index, accepts a same-file alias, rejects a distinct
copy, and keeps Toolhelp as a conflicting-child refusal. Its exact candidate
then reached the first `computer.observe` and failed on the WGC/QPC conversion
boundary. The sanitized [negative record](../evidence/v0.12.60/computer/attempts/withdrawn-7ceb294-windows-wgc-compositor-frame-age/README.md)
is immutable and was not retried. Version 0.12.61 performs subtraction in one
rational QPC domain and rounds positive age upward, but its fresh candidate
still failed when the compositor time led a later user-mode QPC sample. Its
sanitized negative record is retained on branch
`evidence/v0.12.61-windows-wgc-timestamp-ahead-33271808677`. Version 0.12.62
saturates any future lead to zero elapsed age. Its exact Windows attempt is
terminal `candidate-execution-unknown`; the sanitized reservation is preserved
under `evidence/v0.12.62/computer/attempts/`. Version 0.12.68 requires fresh
bytes and evidence.

See [SOTA audit](SOTA_AUDIT.md) and the [evidence index](../evidence/) for current boundaries.

The v0.12.47 candidate passed both repaired macOS lanes and mandatory visual
review, then was withdrawn when the coordinator supplied an empty aggregate
output path to the finalizer. Version 0.12.48 moved that dependent path
construction into the checked-in, self-tested finalizer wrapper, then failed
closed at the 1,000 ms post-receipt evidence ceiling described above. Version
0.12.49 repaired that newly observed timing boundary without changing the
product's three-second stale-frame refusal. Version 0.12.50 retained the repair
and corrected only the Windows readiness probe described above. Version
0.12.59 repaired the terminal v0.12.58 Toolhelp false negative but exposed the
path-alias false negative above. Version 0.12.60 repaired that exact boundary
but exposed the first WGC/QPC conversion failure above. Version 0.12.61
preserved the prior changes and repaired that precision boundary, then exposed
the valid future-lead boundary. Version 0.12.62 repaired that boundary, but its
Windows reservation survived without a conclusive terminal result. Version
0.12.68 preserves all prior changes with fresh release identity.

## Versioning

Every completed extension/package work item or deployment must bump and align:

- the Rust package version;
- `Cargo.lock`;
- the extension manifest version; and
- the extension library version.

Bump every pinned location in one command and audit the result:

```bash
bash scripts/bump-version.sh 1.2.3
bash scripts/audit-versions.sh
```

The pinned locations are declared once in `scripts/version-pins.txt`
(`Cargo.toml`, `extension/manifest.json`, `extension/lib.js`, the workflow
input examples, and the release commands below); `Cargo.lock` is rewritten and
validated with `cargo metadata --locked`. Run `bash scripts/audit-versions.sh`
before packaging. Commit finished work with a Conventional Commits message
unless the current task explicitly says not to commit.

## Deployment

In this repository, `deploy` means finishing the entire release operation, not
merely compiling or starting the bridge. The intended version must be committed
and pushed; one exact green `main` commit must become a frozen cross-platform
candidate; CI-hosted acceptance must pass against those exact bytes on
`windows-latest` and `macos-26`; and only then may those unchanged bytes become
an annotated tag and immutable public GitHub Release. Publication is a
separate, manual workflow so failed candidates do not create tags, Releases, or
protected environment deployments.

### Phase 1: build and accept a tagless release candidate

Run `.github/workflows/deploy.yml` manually from `main`. Before dispatch, ensure
that the version is aligned across the Rust package and extension
(`bash scripts/audit-versions.sh`), the exact source is the current
`origin/main` tip, the required CI jobs are green for that source tree, and
neither `vVERSION` nor a Release with that name exists.

```bash
REPOSITORY="flrngel/local-browser-bridge"
VERSION="0.12.68"
SOURCE_SHA="EXACT_40_CHARACTER_GREEN_MAIN_SHA"

gh workflow run deploy.yml \
  --repo "$REPOSITORY" \
  --ref main \
  -f version="$VERSION" \
  -f source_sha="$SOURCE_SHA"
```

The candidate workflow has six jobs:

1. **Verify** binds the request to the exact green `main` source, runs the
   release and package contract self-tests, packages the extension, and
   attests it.
2. **Windows** builds and verifies the x86_64 MSVC server and helper, then
   attests both executables.
3. **macOS** builds and verifies the universal server and stable helper app
   bundle, then attests the archive.
4. **Assemble** downloads those outputs, creates the canonical
   `SHA256SUMS.txt`, verifies the exact five-file set and exact-source
   attestations, and freezes one `release-candidate` artifact for 14 days.
5. **Acceptance** calls `.github/workflows/acceptance.yml` in `artifact` mode
   on `windows-latest` and `macos-26`. Each runner downloads that exact
   artifact, verifies `SHA256SUMS.txt`, and runs `scripts/ci-acceptance.mjs`
   against the packaged server, helper, and extension (see
   [CI-hosted acceptance](#ci-hosted-acceptance)). The per-OS
   `acceptance-<os>` artifacts keep `acceptance.json`, screenshots, and
   redacted logs for 30 days.
6. **Receipt** combines both `acceptance.json` files into
   `acceptance-receipt.json` (`schemaVersion` 4: version, source SHA, run ID
   and attempt, raw candidate-ZIP SHA-256, checksum-manifest SHA-256, and the
   complete per-OS results), requires every required check to be `pass`,
   attests the file with GitHub build provenance, and uploads it as the
   `acceptance-receipt` artifact.

This phase has no release environment and deliberately creates no tag, draft,
deployment, or Release. A failed acceptance fails the run, so the candidate
cannot be published; fix the source, merge, and dispatch a fresh candidate. A
rerun is a distinct attempt and produces a distinct receipt.

### Phase 2: publish the accepted bytes

Dispatch `.github/workflows/publish.yml` manually from `main`. Rehearse it
first with `dry_run=true`: preflight then stops after the remote-conflict
check without tagging or publishing.

```bash
REPOSITORY="flrngel/local-browser-bridge"
VERSION="0.12.68"
SOURCE_SHA="EXACT_40_CHARACTER_GREEN_MAIN_SHA"
CANDIDATE_RUN_ID="EXACT_GITHUB_RUN_ID"
CANDIDATE_RUN_ATTEMPT="EXACT_GITHUB_RUN_ATTEMPT"

gh workflow run publish.yml \
  --repo "$REPOSITORY" \
  --ref main \
  -f version="$VERSION" \
  -f source_sha="$SOURCE_SHA" \
  -f candidate_run_id="$CANDIDATE_RUN_ID" \
  -f candidate_run_attempt="$CANDIDATE_RUN_ATTEMPT" \
  -f dry_run=true
```

Repeat without `dry_run` to publish. The first publication job runs outside
the protected environment and has no Release-mutation authority. It
reconstructs the candidate from GitHub, raw-hashes the artifact ZIP, verifies
the exact five-file payload and attestations, downloads the
`acceptance-receipt` artifact of the same run, verifies its build-provenance
attestation against `deploy.yml` and the exact source, and requires that the
receipt names this candidate (run, attempt, artifact ID, raw ZIP and manifest
digests) with every required check passed on both platforms before preparing
the immutable publication input.

Only the subsequent `release` job uses the protected `release` environment. It
must consume the exact preflight-approved bytes, recheck the repository and tag
policies, create annotated `vVERSION` for the accepted source, create the
immutable GitHub Release, re-download every published asset, and verify the
downloaded set byte for byte. If the workflow implements a temporary
publication-transfer artifact, cleanup may delete only that exact transfer
artifact. It must not delete the frozen candidate, the acceptance receipt,
unrelated workflow artifacts, a tag, or a Release.

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
