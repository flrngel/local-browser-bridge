# Windows acceptance coordinator source-gate record

- Status date: 2026-08-25
- Source target: 0.12.29
- Source-gate status: **passed under exact 64-bit Windows PowerShell 5.1; independent review found no P0/P1 issue**
- Release status: **no candidate, platform acceptance, tag, or Release has occurred**

This document records the Windows-local engineering resolution of the
acceptance-coordinator tooling defect. It is non-product source evidence, not an
observed Local Browser Bridge product failure and not candidate acceptance. The
0.12.28 source checkpoint never became a candidate. No packaged 0.12.29
candidate has been built, downloaded, or executed.

> **Successor status:** the forward-looking v0.12.29 instructions below are
> retained as historical source-gate context. A later v0.12.30 candidate passed
> both macOS lanes, then failed closed before Windows runner launch because the
> staged worker-support loader referenced an undefined hex helper. Version
> 0.12.31 fixed that loader and passed its quiet macOS lane, but the deliberate
> lane's two-second post-receipt fresh-frame window timed out before product
> dispatch. Version 0.12.32 enlarged that wait and built one trusted candidate,
> but its source-compiled app-share self-test emitted an extra stdout line and
> failed before permission probes, quiet-seat stabilization, or candidate
> process launch. Stock Chrome, tagging, and publication did not run for any of
> those versions. Version 0.12.33 restored and enforced the exact self-test
> output contract and passed both fresh macOS lanes. Its single Windows attempt
> then failed closed at `build-dedicated-fixture` before a fixture executable,
> candidate product process, Chrome action, tag, or publication existed. The
> sanitized terminal record is retained under
> `evidence/v0.12.33/computer/attempts/withdrawn-81bda2a-windows-fixture-build-failure/`.
> Version 0.12.34 kept the strict frame-authority checks, permitted only explicit
> child breakaway from both the detached-worker guard Job and coordinator
> lifetime Job, atomically created each runner-owned child in its private
> kill-on-close Job, and exercised the actual guard-plus-lifetime fixture build
> in source-only self-tests. Its exact trust gate and both fresh macOS lanes
> passed. At `2026-08-26T11:05:46Z`, however, the Windows coordinator created
> its persistent no-retry reservation before any coordinator state existed, and
> the invoking session was interrupted. A bounded later observation found no
> v0.12.34 coordinator directory, evidence directory, candidate process,
> listener on port 17373, Computer Use action, or Chrome action. That absence is
> not proof that an unobserved transient state never existed. The persistent
> ledger records the Windows attempt as `not-started` with retry disabled, so
> v0.12.34 was withdrawn without a tag or GitHub Release.
>
> The v0.12.35 candidate later stopped at the source-only macOS quiet-seat gate
> without executing candidate bytes. The v0.12.36 candidate passed both fresh
> macOS lanes, then its Windows trust verifier failed closed while establishing
> the fresh destination's protected owner-only ACL. Read-only post-failure
> inspection found an ordinary empty destination with its inherited ACL still
> intact. No trust subdirectory, source clone, artifact download, Windows
> reservation, candidate process, Computer Use action, Chrome action, tag, or
> Release followed. The exact internal exception was not retained and is not
> claimed. The sanitized record is frozen at commit
> [`e79db0397333543a4e4d6435868c26f4ff524ce5`](https://github.com/flrngel/local-browser-bridge/tree/e79db0397333543a4e4d6435868c26f4ff524ce5/evidence/v0.12.36/computer/attempts/withdrawn-be22679-windows-trust-private-acl-failure).
>
> The exact v0.12.42 candidate passed its source and artifact trust gates, then
> failed closed in macOS package preparation because the acceptance inventory
> did not yet include the newly shipped `Local Browser Bridge.app` desktop
> host. Candidate bytes were not executed, and Windows or Chrome acceptance
> never started. The sanitized record is retained under
> `evidence/v0.12.42/computer/attempts/withdrawn-014800d-macos-package-inventory-mismatch/`.
>
> Version 0.12.45 aligned the bounded macOS acceptance inventory with the
> shipped desktop-host bundle and passed both macOS lanes plus the corrected
> Windows trust gate. Its one-shot Windows reservation then failed at the first
> packaged executable probe: the shipped GUI host reported
> `local-browser-bridge-desktop 0.12.45`, while native acceptance still required
> the obsolete `local-browser-bridge 0.12.45` server-binary string. Computer Use
> and Chrome never started, no screenshot or release evidence was produced, and
> the candidate was withdrawn without a tag or Release.
>
> Version 0.12.46 aligned that native consumer with the already enforced Windows
> artifact contract. Its exact candidate then stopped in the macOS quiet lane
> before product dispatch because the independent active-receiver watcher
> expired after eight seconds even though the server call boundary remained
> open for fifteen seconds. The fixture click count stayed zero and no Windows
> attempt started. Version 0.12.49 retained the Windows identity repair and
> bound the runner and native receiver probe to one sixteen-second watcher
> deadline. Both fresh macOS lanes passed. Its one-shot Windows run then failed
> before UI use because the readiness probe's redundant Toolhelp basename
> filter returned no exact-image child for the authenticated helper worker.
> Computer Use and Chrome never started, and no screenshot, tag, or Release was
> produced. Version 0.12.50 removed only that advisory filter while retaining
> direct-parent, exact live full-image-path, worker-PID, protocol-session, and
> interactive-session binding. It also retains the Job topology but resolves
> only the prospective
> ledger path before staging. It completes and verifies owner-private staging,
> configuration and Start records, detached-worker lifetime-Job binding and
> guard-ownership transfer, and exact runner process/environment/token
> preparation before atomically creating the schema-2 persistent reservation
> bound to an opaque `coordinatorInstanceId`. That create-once boundary
> immediately precedes runner launch intent and the sole process creation. A
> pre-boundary failure may leave private diagnostic scratch state without
> consuming the candidate. A foreign coordinator's reservation cannot rewrite
> that local pre-boundary history. Once the owning ledger exists, the outcome
> is conservative unknown: never delete it or retry that product version. A
> same-version reservation whose manifest does not match the local coordinator
> is intentionally classified as invalid rather than foreign; `Follow` then
> refuses the ambiguous state instead of projecting a local not-started result.
> Its trust verifier also creates the fresh destination and protected owner-only
> DACL in one operation, verifies the exact persisted owner and single explicit
> rule, and exercises that boundary in its Windows PowerShell 5.1 self-test.
> The v0.12.50 candidate was later withdrawn before Windows execution after a
> coordinator scratch-path error terminated its deliberate macOS runner.
> The exact v0.12.58 candidate passed both macOS lanes and this Windows trust
> gate. Its native runner then failed at `bind-initial-helper-readiness`: the
> authenticated helper session and worker PID stayed stable for 265 polls, but
> the redundant Toolhelp snapshot returned zero exact-image direct children.
> No sentinel, Computer Use click, Chrome action, tag, or Release followed.
> Version 0.12.59 bound the authenticated controller PID to the exact launched
> supervisor and the worker PID to a queried live path and interactive session.
> Its exact candidate passed both macOS lanes and this trust gate, then failed
> before UI use because a valid path alias did not string-equal the candidate
> path. The sanitized
> [negative record](../evidence/v0.12.59/computer/attempts/withdrawn-ece060c-windows-helper-readiness-image-mismatch/README.md)
> contains no absolute path or candidate byte. Version 0.12.60 replaces that
> string comparison with the live image file object's volume serial and file
> index. Toolhelp remains a conflicting-child refusal rather than a required
> rediscovery source. Failed runner summaries still take precedence over the
> generic missing-watcher terminal reason.
> The exact v0.12.60 candidate passed that identity check and the single
> foreground-arm action, then failed closed at its first `computer.observe`
> because WGC/QPC conversion classified a compositor timestamp as outside the
> monotonic range. Stock Chrome never started. Its sanitized
> [negative record](../evidence/v0.12.60/computer/attempts/withdrawn-7ceb294-windows-wgc-compositor-frame-age/README.md)
> is immutable and non-release evidence. Version 0.12.61 kept subtraction in
> one rational QPC domain, but its exact candidate still failed at the first
> `computer.observe` because the WGC compositor timestamp led a later user-mode
> QPC sample by more than that assumed tolerance. Its single foreground-arm
> action completed, stock Chrome never started, and the candidate was not
> retried. Version 0.12.62 preserves and rounds positive age upward while
> saturating every future lead to zero elapsed age at callback receipt.
> Version 0.12.62 requires entirely fresh candidate and acceptance evidence. Do
> not reuse or relabel earlier bytes or results.

## Source-freeze release boundary

At this handoff's source-freeze checkpoint, the coordinator source gate was
complete but every artifact- and UI-bearing release gate remained future work:

- no 0.12.62 tag has been created or pushed;
- no packaged 0.12.62 server, helper, or extension has been built, downloaded,
  or executed;
- neither macOS packaged lane nor Windows packaged-helper acceptance has run;
- stock-Chrome acceptance has not run and Chrome was not opened or mutated;
- no candidate-bound evidence commit, approval, publication, or GitHub Release
  has occurred; and
- do not treat a coordinator marker, `Follow` result, or this handoff as user
  consent or action authority.

The live publication status belongs to the immutable
[v0.12.62 GitHub Release](https://github.com/flrngel/local-browser-bridge/releases/tag/v0.12.62)
and its candidate-bound schema-3 evidence receipt, not to this source-checkpoint
list.

Coordinator self-tests are GUID-scoped non-product tests and may be repeated.
Their pass does not freeze or authorize candidate bytes. Any future packaged
candidate attempt remains one-shot after its schema-2 reservation or any
outcome-unknown launch.

## Verified facts

The active 0.12.62 source provides a checked-in
`scripts/run-windows-computer-use-acceptance.ps1` coordinator with:

- a clean exact-system-PowerShell bootstrap;
- fixed, owner-private LocalAppData roots and a schema-2 per-version attempt
  reservation bound to an opaque coordinator-instance identifier;
- exact source, candidate, manifest, and helper bindings;
- an explicit worker environment allowlist with an in-memory token handoff;
- atomic create-once Start, ownership, intent, runner, watcher, handoff,
  failure, and final records;
- separate persistent worker, runner, and watcher streams;
- suspended native process creation with an exact handle list and Job list;
- exact PID plus process-start-time liveness checks;
- full predecessor-chain validation for `Follow`;
- watcher startup only after the foreground-arm request exists; and
- notification-only `Follow` output with `uiActionAllowed: false`.

It now also provides the reviewed fail-closed named-Job state machine described
in [Implemented state machine](#implemented-state-machine).

Before the Windows-local repair, the following source and non-Windows checks had
passed against the historical 0.12.28 checkpoint:

| Check | Result |
|---|---:|
| PowerShell AST parse of the coordinator | PASS |
| `cargo test --locked --test computer_contract` | PASS, 51/51 |
| Browser evidence, extension, and release contract tests | PASS, 136/136 |
| Extension JavaScript syntax checks | PASS |
| macOS finalizer and handoff watcher self-tests | PASS |
| v0.12.28 packaged-evidence rig self-test | PASS |
| Swift fixture and handoff typechecks | PASS |
| Compiled macOS app-share handoff self-test | PASS |
| Candidate-attestation selection self-test | PASS |
| Release-evidence verifier self-test | PASS |

Those historical checks did not replace Windows PowerShell 5.1 or native Job
Object execution. The later 0.12.29 native result is recorded below.

## Historical 0.12.28 Windows result

One non-product self-test was run through the authorized Windows machine's
actual 64-bit Windows PowerShell 5.1 host. The transferred script was 218,966
bytes with SHA-256:

```text
87c8640257958ee9dcecf7190a9d309eaf918b83a1e3266d3a9eb9b5f7001b47
```

It failed before any candidate, server, helper, browser, or UI action with:

```text
Exception calling ".ctor" with "4" argument(s):
"The recovered coordinator lifetime Job did not become a fresh object"
```

The failure is useful native evidence: `TerminateJobObject` followed by
`JOBOBJECT_BASIC_ACCOUNTING_INFORMATION.ActiveProcesses == 0` does not prove
that the dying prior owner has finished closing its final Job handle or that
the named kernel object has left the namespace.

The exact transferred test files and their download temporaries were removed
afterward. The temporary Mac HTTP server was stopped. No product process or
loopback listener was started by this self-test.

## Historical provisional source

The abandoned 0.12.28 coordinator source had SHA-256:

```text
d4f5b3edd13741f52613995a06bfaba85e1b8f42bb9ce2c897b24e31e61cc930
```

Its historical Rust contract file had SHA-256:

```text
3117da52da6858c59992c77630e9ae105f36b1a5da2c315544bafa4983d1c45a
```

Before this Windows-local work, that source had **not** been executed on
Windows. At the start of this task, its exact PowerShell 5.1 parser and then-
incomplete SelfTest both passed, including the required one-line result. That
baseline pass did not exercise delayed namespace disappearance, a retained
extra handle, or the final same-name create race, so it did not resolve the
known source gate. The provisional source added:

- a shared monotonic recovery deadline;
- bounded retries when `CreateJobObject` still reports
  `ERROR_ALREADY_EXISTS`; and
- a live pre-transfer guard-close probe that retains an exact process handle,
  closes the launcher's Job without transferring it, and requires bounded
  worker termination.

It was never promoted into a Windows pass or candidate.

## Authoritative source binding

The native source-gate proof is bound to the following immutable inputs:

| Binding | Value |
|---|---|
| Implementation commit | `b316eeb8ec6ed460c161f4c8858ae7b31c551641` |
| Coordinator SHA-256 | `c2a983109e450d85a1b17c4da1b19aa2158ed94eba516d887246365da567a6c2` |
| Rust contract SHA-256 | `c03beec573706352fefe82eccae80757e28d3fd4814f9fe6266a432b459d04f8` |
| Exact-head GitHub Actions run | [`32820442002`](https://github.com/flrngel/local-browser-bridge/actions/runs/32820442002) |
| Windows host contract | Exact system Windows PowerShell 5.1 Desktop, 64-bit process |

Run `32820442002` checked out the exact implementation commit and passed all
five CI jobs. Its Windows job parsed and executed the coordinator through the
exact system Windows PowerShell 5.1 host, accepted only exit code zero and the
single exact success line shown below, and then passed the Windows compiler,
test, and artifact-policy lane. The coordinator independently rejects a
non-system or non-64-bit host before running the scenarios.

This GitHub-hosted exact-head execution is the authoritative, source-bound
native proof for removing the source-only `RELEASE_BLOCKED` marker. The earlier
FLRngel19 execution informed the repair but is not being represented as a
retained candidate or product-acceptance artifact. The hashes above cover the
executable test source and its Rust contract; later documentation-only or CI
hardening commits do not change those tested bytes. Pull-request and post-merge
CI must still pass before this work is merged.

The CI workflow now additionally rejects a dirty, partially materialized, or
structurally invalid Windows checkout by checking porcelain status, staged and
unstaged diffs, deleted and untracked inventories, every tracked path, and
`git fsck --full`. This source-integrity check is a merge gate, not packaged
candidate acceptance.

## Implemented state machine

Version 0.12.29 removes the abandoned `CreateFreshJob` retry. The reviewed
implementation is fail closed:

1. Acquire the stable session admission mutex before all Job recovery work.
2. Start one monotonic deadline for the entire recovery operation.
3. Open the prior stable named Job with only the required query and terminate
   rights. If none exists, continue to step 7.
4. Terminate that exact Job, query it until `ActiveProcesses == 0`, and close
   the inspected handle exactly once.
5. Poll `OpenJobObject` under the same deadline until the name returns
   `ERROR_FILE_NOT_FOUND`. Close every successful poll handle before sleeping.
   This observes namespace release without adopting an existing object.
6. Clip every sleep to the remaining deadline. A stream of existing or raced
   objects must not renew the timeout.
7. Call native `SetLastError(0)`, call `CreateJobObject` once, and immediately
   capture `Marshal.GetLastWin32Error()` before any other P/Invoke.
8. Accept only a non-null handle with last error zero. Transfer it into the
   lifetime field only in that branch.
9. For a non-null handle with `ERROR_ALREADY_EXISTS` or any other nonzero last
   error, close it exactly once and fail closed. Never configure, assign,
   retain, terminate, or retry that uninspected object.
10. Configure `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, bind the worker, and only
    then publish Worker or Intent state.

The embedded C# remains compatible with Windows PowerShell 5.1 and .NET
Framework. Observations never renew the deadline, and every poll sleep is
clipped to the remaining monotonic budget.

## Windows-native self-test result

The exact source was parsed and run through the exact 64-bit system Windows
PowerShell 5.1 Desktop host. Its sole output was:

```text
Windows computer-use acceptance coordinator self-test passed.
```

| GUID-scoped native scenario | Result | Proven behavior |
|---|---:|---|
| Normal clean start | PASS | No prior name; one fresh Job was configured and bound. |
| Live prior tree | PASS | Exact prior owner and descendant handles signaled before recovery returned. |
| Delayed external handle | PASS | Recovery waited through zero active members until the non-member handle closed and the namespace name disappeared. |
| External-handle timeout | PASS | One shared deadline bounded a fail-closed timeout and returned no bound Job. |
| Create race | PASS | A same-name object created after absence was closed and refused at the single final create; it was not adopted or terminated. |
| Pre-transfer guard close | PASS | Closing the launcher's guard terminated the exact worker in bounds. |
| Transferred guard | PASS | Launcher disposal did not terminate the worker; worker exit killed its Job-owned descendant and left the unrelated control process alive. |
| Stream cleanup | PASS | Self-test stdout/stderr became exclusively openable and only self-test-owned paths were removed. |

Every Job and mutex name used by these scenarios was unique and GUID-scoped.
The tests did not touch the production coordinator names, start a product
binary, open a browser, access a network listener, or open UI. Exact process
handles were retained wherever PID reuse would make PID-only proof ambiguous.
Independent review found no P0/P1 issue in native handle ownership, Job
identity, deadline behavior, cleanup, publication ordering, or release gating.

## Base Windows development environment

Use native 64-bit Windows tooling, not WSL, because this work exercises Win32
process, mutex, Job Object, ACL, and PowerShell 5.1 behavior. Install only from
the Microsoft WinGet catalog or the vendors' official download pages. Do not
disable Defender, SmartScreen, UAC, execution-policy enforcement, or repository
security checks.

From an elevated PowerShell used only for machine-level tool installation:

```powershell
winget source update
winget install --exact --id Git.Git --source winget `
  --accept-package-agreements --accept-source-agreements
winget install --exact --id Microsoft.VisualStudio.2022.BuildTools --source winget `
  --accept-package-agreements --accept-source-agreements `
  --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
winget install --exact --id OpenJS.NodeJS.LTS --source winget `
  --accept-package-agreements --accept-source-agreements
winget install --exact --id Rustlang.Rustup --source winget `
  --accept-package-agreements --accept-source-agreements
winget install --exact --id GitHub.cli --source winget `
  --accept-package-agreements --accept-source-agreements
```

Package identifiers can change. If an exact identifier is unavailable, use
`winget search <name> --source winget` and select the vendor's stable package;
do not substitute a similarly named third-party installer. The required Visual
Studio component is **Desktop development with C++**, including the x64/x86
MSVC tools and a supported Windows 11 SDK. Restart the terminal after installs,
then pin the repository's Rust toolchain:

```powershell
rustup toolchain install 1.88.0 --profile minimal --component rustfmt,clippy
rustup default 1.88.0
rustup target add x86_64-pc-windows-msvc

git --version
node --version
rustc --version
cargo --version
gh --version
```

Use Node.js 24 for parity with repository CI. GitHub CLI is needed only for
authenticated fetch/push operations; use its secure credential store and never
print or persist a token in logs. Python is not required.

Clone to a short fixed local path so the retained evidence tree materializes:

```powershell
New-Item -ItemType Directory -Path C:\src -Force | Out-Null
git -c core.longpaths=true clone `
  https://github.com/flrngel/local-browser-bridge.git `
  C:\src\local-browser-bridge
Set-Location C:\src\local-browser-bridge
git config --local core.longpaths true
git fetch --tags origin
git status --porcelain=v2 --untracked-files=all
git diff --quiet HEAD --
git diff --cached --quiet
git fsck --full
```

Before editing, require a clean worktree and confirm the expected handoff commit
from the coordinator who supplied the prompt. Do not reuse an earlier long-path
checkout with missing tracked PNGs.

## Windows-local validation recipe

Use a fresh short-path checkout on an NTFS or ReFS fixed local volume. Enable
long paths for the checkout before materializing tracked evidence files. Start
from a clean detached commit or a clean local branch and record its full SHA.

First prove the exact system host. Do not derive it from a Machine-scoped
`SystemRoot`: that variable can be absent even when the native host is present.
Resolve it from the runtime-provided system directory and validate the child
identity:

```powershell
$ErrorActionPreference = "Stop"
if (-not [Environment]::Is64BitProcess) {
  throw "The validation launcher must be a native 64-bit Windows process."
}
$systemDirectory = [Environment]::SystemDirectory
if ([String]::IsNullOrWhiteSpace($systemDirectory) -or
    -not [IO.Path]::IsPathRooted($systemDirectory)) {
  throw "The native Windows system directory is unavailable."
}
$ps51 = [IO.Path]::Combine(
  [IO.Path]::GetFullPath($systemDirectory),
  "WindowsPowerShell",
  "v1.0",
  "powershell.exe"
)
if (-not [IO.File]::Exists($ps51)) {
  throw "The exact system Windows PowerShell executable is unavailable."
}
$identity = & $ps51 -NoLogo -NoProfile -NonInteractive -Command `
  '"{0}.{1}|{2}|{3}" -f $PSVersionTable.PSVersion.Major, $PSVersionTable.PSVersion.Minor, $PSVersionTable.PSEdition, [Environment]::Is64BitProcess'
if ($LASTEXITCODE -ne 0 -or $identity -cne "5.1|Desktop|True") {
  throw "The exact 64-bit system Windows PowerShell 5.1 identity is unavailable."
}
```

The required result is exactly:

```text
5.1|Desktop|True
```

Then parse and self-test the coordinator through that same executable:

```powershell
$coordinator = (Resolve-Path `
  ".\scripts\run-windows-computer-use-acceptance.ps1").Path

$parse = @'
$tokens = $null
$errors = $null
[Management.Automation.Language.Parser]::ParseFile(
  [IO.Path]::GetFullPath($env:LBB_COORDINATOR_PARSE_PATH),
  [ref]$tokens,
  [ref]$errors
) | Out-Null
if ($errors.Count -ne 0) {
  throw (($errors | ForEach-Object { $_.Message }) -join [Environment]::NewLine)
}
'@

$env:LBB_COORDINATOR_PARSE_PATH = $coordinator
try {
  & $ps51 -NoLogo -NoProfile -NonInteractive -Command $parse
  if ($LASTEXITCODE -ne 0) { throw "Coordinator parse failed." }

  $output = @(& $ps51 -NoLogo -NoProfile -NonInteractive `
    -File $coordinator -Mode SelfTest)
  if ($LASTEXITCODE -ne 0 -or
      $output.Count -ne 1 -or
      $output[0] -cne `
        "Windows computer-use acceptance coordinator self-test passed.") {
    throw "Coordinator self-test did not return its exact success result."
  }
}
finally {
  Remove-Item Env:\LBB_COORDINATOR_PARSE_PATH -ErrorAction SilentlyContinue
}
```

Do not install Node.js or Rust merely to run this Windows self-test. Repository
CI remains responsible for the Rust, extension, macOS, and release-contract
matrix. If Rust is already available from a trusted installation, also run:

```powershell
cargo test --locked --test computer_contract
```

Record only sanitized output: commit SHA, coordinator SHA-256, exact pass/fail,
first internal error message and stack frame, cleanup state, and whether any
product process or listener existed. Do not retain tokens, usernames, home
paths, command-line secrets, signed URLs, or environment identifiers.

## Completed source-unblock criteria

The reviewed 0.12.29 source satisfies the Windows-local unblock criteria:

- PASS: the uninspected same-name race is closed and refused after exactly one
  final create;
- PASS: the exact 64-bit system Windows PowerShell identity is
  `5.1|Desktop|True` and the parser accepts the coordinator;
- PASS: the exact one-line coordinator SelfTest passes all eight native
  scenarios;
- PASS: the focused 0.12.29 source-contract matrix passes 192/192;
- PASS: self-test cleanup leaves no owned process, listener, or locked stream;
- PASS: independent review reports no P0/P1 coordinator finding; and
- PASS: documentation and static contracts describe the implemented state
  machine rather than the abandoned retry.

## Historical future-candidate checklist (superseded)

The source pass permits later candidate preparation; it does not perform or
approve it. At this source-gate checkpoint, the next release task was required
to use version 0.12.29 consistently and not reuse 0.12.28 candidate bytes or
evidence. The successor-status note at the top now governs the current cycle;
this historical checklist was, in order:

1. freeze and independently verify one exact GitHub-attested five-file
   candidate;
2. run both fresh, non-mergeable macOS packaged lanes;
3. run one Windows packaged native-helper acceptance attempt;
4. run one stock-Chrome extension acceptance attempt;
5. commit sanitized candidate-bound evidence and the canonical receipt;
6. obtain protected publication approval;
7. verify the immutable five-asset GitHub Release and updater; and
8. atomically refresh `dist/` from downloaded published assets.

None of those steps occurred during this source-unblock task. No product
process or listener remained after the non-product SelfTest, and no packaged
candidate or Chrome action occurred.
