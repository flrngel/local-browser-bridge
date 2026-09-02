# Windows computer-use fixture

`WindowsComputerUseFixture.ps1` is the source for a deterministic, fixture-owned target used by live Windows helper acceptance. The repository runner hashes that exact source, invokes the exact system Windows PowerShell 5.1 Desktop host to compile it into a new runner-owned temporary `.exe` with C# `WindowsApplication` output, runs the executable's entry-point self-test, and then launches the same executable directly. The compiled fixture is ephemeral acceptance tooling, not a release asset. A live acceptance UI hosted by `powershell.exe`, a terminal, or a console process does not satisfy this boundary.

The executable accepts only the case-sensitive ordinal grammar `--self-test` or `--evidence-directory DRIVE_ABSOLUTE_NON_ROOT_PATH [--show-occluder]`. It rejects relative, drive-relative, root-relative, drive-root, and UNC paths; unknown, reordered, duplicated, or differently cased switches; and an evidence directory in which any of the three protected fixture records already exists. The source hash must match before and after compilation. The executable hash is recorded after compilation and must remain unchanged through its entry-point self-test and initial live process binding.

Before creating UI, the executable assigns the stable AppUserModelID `LocalBrowserBridge.WindowsAcceptance`. This is fixture identity metadata only. It does not grant consent, identify the exact runner instance, or provide acceptance authority. The runner separately requires one exact-image fixture process in its interactive session, the sole exact-image direct child of the runner, and the same PID in the fixture's create-once ready file.

Use `-ShowOccluder` to place a no-activate magenta window over the animated target. An exact-window capture must not include those magenta pixels.

The target, sentinel, occluder, and backdrop are nonactivating top-level
windows. The orange sentinel is a disabled status surface that says no action
is required; it never accepts or authorizes a setup click. Its window procedure
passively counts left-button attempts, including child notifications, so any
sentinel click invalidates the run. All legacy arm request and acknowledgement
counters also remain zero for the whole run. WinForms can report a thread-local
`Activated` callback even while another process still owns the OS-global
foreground window, so the retained target and sentinel activation counters
increment only when `GetForegroundWindow()` equals that exact fixture HWND.

After exact fixture and helper binding, the runner creates the sanitized
schema-v3 `operator/foreground-arm-request.json` automatic notification. It
then accepts three distinct, advancing fixture publications only while one
unchanged foreground/focus root belongs to the runner's current interactive
session but not to the fixture process. Each sample must also prove a native foreground-before/after
seqlock, stable owner identity, matching focus root, unchanged global cursor,
unchanged input desktop, and zero fixture input. The runner injects no input
and changes no focus or cursor to establish this baseline. It then creates the
matching schema-v3 `operator/foreground-arm-received.json` ready notification
and proves one more fresh publication plus accepted-sample-to-baseline
continuity before any product action.

Every state write carries a monotonic publication generation. The runner
requires distinct advancing publications for the initial zero-input boundary,
each stable native sample, the bound baseline, and every later invariant
comparison; one stale valid JSON snapshot cannot satisfy the live proof.

Compile the embedded C# and exercise its zero-input and nonactivation contracts
under the exact system Windows PowerShell 5.1 host without opening fixture
windows or creating evidence. CI also compiles and runs the in-memory self-test
under PowerShell 7, then builds a source-hash-bound
temporary `WindowsApplication`, executes its strict `--self-test` entry point,
and removes it. PowerShell 7 remains a compatibility self-test surface rather
than the live WinForms/.NET Framework acceptance runtime:

```powershell
$windowsPowerShell = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File `
  .\tests\fixtures\windows\WindowsComputerUseFixture.ps1 -SelfTest
```

The fixture writes only these files beneath the caller-selected directory:

- `fixture-ready.json`: process and fixture-owned HWND identifiers;
- `fixture-state.json`: current counters, fixture-owned bounds and handles, and SHA-256/length text proofs; it does not retain the external foreground handle or global cursor coordinates;
- `fixture-events.ndjson`: fixture-owned input events, non-character message parameters, redacted character messages, and decoded key `lParam` bits.

Character contents, tokens, environment variables, command lines, executable paths, and user-profile paths are never written. Existing evidence files are never replaced. Closing the target closes the test-owned sentinel and optional occluder; the target UI thread also closes its capture-evidence backdrop.

After the private Job Object has stopped every runner-owned process and the fixture process handle is closed, cleanup inspects only the exact runner-owned build directory. It refuses reparse points or any entry other than the expected executable, deletes that executable, and removes the now-empty directory nonrecursively. It never broadens cleanup or recursively removes an unexpected tree. Any failure leaves the run invalid. The sanitized `summary.json` records the source and executable hashes, source/executable stability, entry-point self-test, exact image/session/direct-child/ready-PID checks, direct Windows-application execution, and successful executable removal without retaining a path.

CI-hosted acceptance (`scripts/ci-acceptance.mjs`) builds this fixture with the system Windows PowerShell 5.1 host, launches it with `--evidence-directory`, and verifies helper actions through `fixture-state.json` (`invokeCount`, `semanticValue`, `focusedText`, `messageCounters`). See [Development](../../../docs/DEVELOPMENT.md#ci-hosted-acceptance).
