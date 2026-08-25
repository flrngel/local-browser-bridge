# Windows acceptance handoff

- Status date: 2026-08-24
- Source target: 0.12.28
- Release status: **blocked; do not tag, execute a release candidate, open stock Chrome, or publish a Release**

This document is the Windows-local engineering handoff for the acceptance
coordinator. It describes a tooling defect, not an observed Local Browser
Bridge product failure. The 0.12.28 product candidate has not been frozen or
executed.

## Stop boundary

Until every item in [Release-unblock criteria](#release-unblock-criteria) is
satisfied:

- do not create or push a 0.12.28 tag;
- do not download or execute a packaged 0.12.28 server or helper;
- do not start the native helper acceptance runner;
- do not load or mutate the extension in stock Chrome;
- do not approve or publish a GitHub Release; and
- do not treat a coordinator marker, `Follow` result, or this handoff as user
  consent or action authority.

The checked-in `RELEASE_BLOCKED` marker makes both the tag-triggered GitHub
release workflow and local `scripts/deploy.sh` fail before building artifacts.
Remove that marker only in the reviewed commit that satisfies every unblock
criterion below.

Coordinator self-tests are non-product tests and may be repeated. A packaged
candidate attempt may not be repeated after a start or outcome-unknown launch.

## Verified facts

The repository changes before this handoff provide a checked-in
`scripts/run-windows-computer-use-acceptance.ps1` coordinator with:

- a clean exact-system-PowerShell bootstrap;
- fixed, owner-private LocalAppData roots and per-version attempt reservation;
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

The following checks passed on macOS against the source in this handoff:

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

These are source and non-Windows checks. They do not replace Windows
PowerShell 5.1 or native Job Object execution.

## Exact Windows result

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

## Current provisional source

The coordinator source present with this handoff has SHA-256:

```text
d4f5b3edd13741f52613995a06bfaba85e1b8f42bb9ce2c897b24e31e61cc930
```

Its Rust contract file has SHA-256:

```text
3117da52da6858c59992c77630e9ae105f36b1a5da2c315544bafa4983d1c45a
```

This newer source was **not** executed on Windows. It adds:

- a shared monotonic recovery deadline;
- bounded retries when `CreateJobObject` still reports
  `ERROR_ALREADY_EXISTS`; and
- a live pre-transfer guard-close probe that retains an exact process handle,
  closes the launcher's Job without transferring it, and requires bounded
  worker termination.

Do not promote that static/local success into a Windows pass.

## Remaining design issue

The provisional `CreateFreshJob` loop closes every handle returned with
`ERROR_ALREADY_EXISTS`, but it retries merely because this constructor had
previously inspected and terminated a same-name Job. An object observed after
the inspected handle was closed is not provably the same object; it could be a
newly raced same-name Job. Cooperative coordinators are serialized by the
mutex, and hostile same-account code is outside the documented sandbox
boundary, but release tooling should still fail closed instead of silently
classifying an uninspected object as the prior one.

The recommended state machine is:

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
   retain, or retry that uninspected object.
10. Configure `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, bind the worker, and only
    then publish Worker or Intent state.

An alternative implementation is acceptable only if it supplies equivalent
kernel-object identity proof and never adopts an existing Job as fresh. Keep
the embedded C# compatible with Windows PowerShell 5.1 and .NET Framework: no
newer C# syntax or runtime-only APIs.

## Required Windows-native self-tests

Run these against the exact source bytes being proposed:

1. **Normal clean start:** no prior Job; fresh Job creation and binding pass.
2. **Live prior tree:** a prior owner plus descendant occupy the named Job;
   recovery terminates both and returns only after both exact process handles
   signal.
3. **Delayed external handle:** a non-member process retains an extra handle
   after the tree reaches zero, then releases it after a bounded delay;
   recovery must wait for namespace absence and then create a fresh Job.
4. **External-handle timeout:** retain that handle beyond a short test
   deadline; recovery must throw within the one shared deadline and must not
   return a bound Job.
5. **Create race:** create another same-name Job between absence observation
   and the final create; the coordinator must close and refuse the returned
   existing handle, never adopt or terminate it as the new lifetime Job.
6. **Pre-transfer guard close:** start a long-lived worker with no lifetime Job
   of its own, retain an exact independent process handle, dispose the launcher
   without `TransferGuardOwnership`, and prove bounded exact termination.
7. **Transferred guard:** after transfer, launcher disposal must not terminate
   the intended worker; worker exit must still terminate its Job-owned
   descendant without affecting an unrelated control process.
8. **Stream cleanup:** every failure path must leave stdout/stderr paths
   unlockable and delete only self-test-owned files.

The self-test must use unique GUID-scoped Job and mutex names. It must never
touch the production coordinator name, start a product binary, or open UI.

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
winget install --exact --id Microsoft.VisualStudio.BuildTools --source winget `
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

First prove the exact system host:

```powershell
$systemRoot = [Environment]::GetEnvironmentVariable("SystemRoot", "Machine")
$ps51 = [IO.Path]::Combine(
  $systemRoot,
  "System32",
  "WindowsPowerShell",
  "v1.0",
  "powershell.exe"
)
& $ps51 -NoLogo -NoProfile -NonInteractive -Command `
  '"{0}.{1}|{2}|{3}" -f $PSVersionTable.PSVersion.Major, $PSVersionTable.PSVersion.Minor, $PSVersionTable.PSEdition, [Environment]::Is64BitProcess'
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

## Release-unblock criteria

All of the following are required before returning to candidate work:

- the remaining Job-name race is resolved or rejected by an equally strict,
  documented design;
- the exact system PowerShell 5.1 parser passes;
- the exact one-line coordinator self-test passes, including all eight native
  scenarios above;
- the checkout is clean and every required source file is materialized;
- the full repository CI matrix passes on the same commit;
- independent review reports no open P0 or P1 coordinator finding; and
- documentation and static contracts describe the implemented behavior rather
  than the abandoned retry design.

The same reviewed unblock commit must remove `RELEASE_BLOCKED`, remove or
replace the temporary
`v01228_release_is_blocked_until_the_windows_handoff_is_resolved` contract in
`tests/release_contract.rs`, and revise every blocked/provisional status line.
A tag whose source still contains the marker is expected to fail before
artifact builds.

If the Windows-local fix becomes a separate completed packaging work item,
advance every package, helper, extension, evidence-harness, workflow, and
documentation version consistently to the next unused version before tagging.
Do not reuse 0.12.28 evidence or candidate bytes for that version.

Only after those source gates pass may the normal release sequence resume:
freeze one exact GitHub-attested five-file candidate, run both fresh macOS
lanes, run one Windows native-helper attempt, run one stock-Chrome extension
attempt, commit sanitized evidence and the canonical receipt, approve the
protected publication, verify the immutable five-asset Release and updater,
and atomically refresh `dist/`.

## Copy-paste task for the Windows-local agent

```text
Set up the native Windows development environment in
docs/WINDOWS_ACCEPTANCE_HANDOFF.md, then read the whole document. Work only on the Windows
acceptance coordinator and its contracts. Do not tag, build, download, or run a
product candidate; do not open or mutate Chrome. Reproduce the coordinator
self-test under exact 64-bit system Windows PowerShell 5.1, replace the
uninspected ERROR_ALREADY_EXISTS retry with a fail-closed fresh-name state
machine, implement the required delayed-handle, timeout, create-race,
pre-transfer guard-close, transferred-guard, and stream-cleanup native probes,
and rerun until the exact one-line self-test passes. Keep one monotonic recovery
deadline, close every native handle exactly once, publish no attempt state
before a fresh Job is configured and bound, and preserve sanitized failure
evidence. Report the full source SHA, coordinator SHA-256, exact test table, and
cleanup state. Stop before any packaged candidate or Chrome action.
```
