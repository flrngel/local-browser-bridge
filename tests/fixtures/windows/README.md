# Windows computer-use fixture

`WindowsComputerUseFixture.ps1` is the source for a deterministic, fixture-owned target used by live Windows helper acceptance. The repository runner hashes that exact source, invokes the exact system Windows PowerShell 5.1 Desktop host to compile it into a new runner-owned temporary `.exe` with C# `WindowsApplication` output, runs the executable's entry-point self-test, and then launches the same executable directly. The compiled fixture is ephemeral acceptance tooling, not a release asset. A live acceptance UI hosted by `powershell.exe`, a terminal, or a console process does not satisfy this boundary.

The executable accepts only the case-sensitive ordinal grammar `--self-test` or `--evidence-directory DRIVE_ABSOLUTE_NON_ROOT_PATH [--show-occluder]`. It rejects relative, drive-relative, root-relative, drive-root, and UNC paths; unknown, reordered, duplicated, or differently cased switches; and an evidence directory in which any of the three protected fixture records already exists. The source hash must match before and after compilation. The executable hash is recorded after compilation and must remain unchanged through its entry-point self-test and initial live process binding.

Before creating UI, the executable assigns the stable AppUserModelID `LocalBrowserBridge.WindowsAcceptance`. This is app-share discoverability metadata only. It does not grant consent, identify the exact runner instance, or provide acceptance authority. The runner separately requires one exact-image fixture process in its interactive session, the sole exact-image direct child of the runner, and the same PID in the fixture's create-once ready file.

Use `-ShowOccluder` to place a no-activate magenta window over the animated target. An exact-window capture must not include those magenta pixels.

The fixture's orange foreground sentinel does not activate itself. After the
repository runner binds the exact target, it posts one fresh test-only arm
generation. The runner waits for the fixture's separate processed-generation
and button-enabled receipt before it prompts for input. The button
accepts one left-button down/up only while the sentinel is the native
foreground root and the exact button owns focus. It records total left-button
attempt counts so an extra click invalidates the acceptance run. A trusted
human or separately authorized Computer Use surface performs that setup click,
then stops interacting while the product actions run.

For remote coordination, start the retained runner and use
`scripts/wait-windows-foreground-arm-handoff.ps1 -Mode Watch` to wait for the
fresh schema-2 action request. Do not initialize, prewarm, or list the Windows
Computer Use app share before that handoff. The correct order is: let the
dedicated fixture process create the sentinel, let the runner bind its exact
image/session/PID/ready-file topology and publish the fresh action marker, and
only then initialize the separately authorized app share and list its windows.
This ensures discovery observes the dedicated GUI that belongs to the current
run rather than an inventory captured before it existed.

The watcher is read-only and binds the exact evidence directory plus runner
PID/start time. Its one sanitized handoff prefers
`windows-computer-use-app-share` and names `human-on-windows-session` as
fallback, but grants neither consent nor acceptance authority. The stable AUMID
may make the application easier to find; it is not a substitute for fresh
visual confirmation of the exact orange **LBB Foreground Sentinel** and
**CLICK TO ARM** button. The external surface still needs fresh one-shot
authorization. Click at most once only when that fresh state is visible; if it
shows **ARMED** or is ambiguous, click zero times. Never retry an unknown
outcome. The fixture's native click receipt and advancing samples remain the
only acceptance proof.

Every state write carries a monotonic publication generation. The runner
requires distinct advancing publications for the click acknowledgement, each
stable native sample, the baseline, and every later invariant comparison; one
stale valid JSON snapshot cannot satisfy the live proof.

Compile the embedded C# and exercise its pure arm-generation/counter state
machine under the exact system Windows PowerShell 5.1 host without opening
fixture windows or creating evidence. CI also builds a source-hash-bound
temporary `WindowsApplication`, executes its strict `--self-test` entry point,
and removes it. PowerShell 7 remains a parser surface rather than the live
WinForms/.NET Framework acceptance runtime:

```powershell
$windowsPowerShell = Join-Path $env:SystemRoot "System32\WindowsPowerShell\v1.0\powershell.exe"
& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File `
  .\tests\fixtures\windows\WindowsComputerUseFixture.ps1 -SelfTest
```

The fixture writes only these files beneath the caller-selected directory:

- `fixture-ready.json`: process and fixture-owned HWND identifiers;
- `fixture-state.json`: current counters, bounds, foreground/system-cursor probes, and SHA-256/length text proofs;
- `fixture-events.ndjson`: fixture-owned input events, non-character message parameters, redacted character messages, and decoded key `lParam` bits.

Character contents, tokens, environment variables, command lines, executable paths, and user-profile paths are never written. Existing evidence files are never replaced. Closing the target closes the test-owned sentinel and optional occluder; the target UI thread also closes its capture-evidence backdrop.

After the private Job Object has stopped every runner-owned process and the fixture process handle is closed, cleanup inspects only the exact runner-owned build directory. It refuses reparse points or any entry other than the expected executable, deletes that executable, and removes the now-empty directory nonrecursively. It never broadens cleanup or recursively removes an unexpected tree. Any failure leaves the run invalid. The sanitized `summary.json` records the source and executable hashes, source/executable stability, entry-point self-test, exact image/session/direct-child/ready-PID checks, direct Windows-application execution, and successful executable removal without retaining a path.

Use the repository-level `scripts/test-windows-computer-use.ps1` runner for live acceptance. It first binds canonical server/helper filenames and versions to one independently hashed frozen-candidate manifest and the trust wrapper's exact `candidate-binding.json`. Its retained schema-2 summary therefore names the source, annotated tag, workflow run and attempt, artifact, raw artifact ZIP, manifest, attested assets, and dedicated fixture process binding that were actually exercised. It launches every child inside a private kill-on-close Windows Job Object, drives the authenticated loopback API, captures exact-target screenshots, checks independent non-interruption probes, and performs exact descendant/share/listener/build cleanup even after a failed assertion. The dedicated executable adds no retained file: a successful `-Suite All -ShowOccluder` run still contains exactly 88 files. See [Development](../../../docs/DEVELOPMENT.md#deterministic-windows-live-acceptance).
