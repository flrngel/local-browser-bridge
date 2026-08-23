# Windows computer-use fixture

`WindowsComputerUseFixture.ps1` is a deterministic, fixture-owned target for live Windows helper acceptance. It includes a topmost `#101820` backing mat beyond the target so the acceptance runner can retain a narrowly cropped desktop-level capture-indicator proof without retaining unrelated desktop pixels. It must be launched with an explicit, new evidence directory:

```powershell
.\tests\fixtures\windows\WindowsComputerUseFixture.ps1 -EvidenceDirectory $temporaryEvidence
```

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

For v0.12.9 remote coordination, reserve that external surface before starting
the runner and use `scripts/wait-windows-foreground-arm-handoff.ps1 -Mode Watch`
to wait for the fresh schema-2 request. The watcher is read-only and binds the
exact evidence directory plus runner PID/start time. Its one sanitized handoff
prefers `windows-computer-use-app-share` and names
`human-on-windows-session` as fallback, but grants neither consent nor
acceptance authority. The external surface still needs fresh one-shot
authorization and a fresh visual confirmation. Click at most once only when the
exact orange action-required window visibly shows **CLICK TO ARM**; if it shows
**ARMED** or is ambiguous, click zero times. Never retry an unknown outcome.
The fixture's native click receipt and advancing samples remain the only
acceptance proof.

Every state write carries a monotonic publication generation. The runner
requires distinct advancing publications for the click acknowledgement, each
stable native sample, the baseline, and every later invariant comparison; one
stale valid JSON snapshot cannot satisfy the live proof.

Compile the embedded C# and exercise its pure arm-generation/counter state
machine under Windows PowerShell 5.1 without opening fixture windows or
creating evidence. CI also parser-checks the script with PowerShell 7, while
the embedded WinForms/.NET Framework fixture runs on its actual acceptance
runtime:

```powershell
.\tests\fixtures\windows\WindowsComputerUseFixture.ps1 -SelfTest
```

The fixture writes only these files beneath the caller-selected directory:

- `fixture-ready.json`: process and fixture-owned HWND identifiers;
- `fixture-state.json`: current counters, bounds, foreground/system-cursor probes, and SHA-256/length text proofs;
- `fixture-events.ndjson`: fixture-owned input events, non-character message parameters, redacted character messages, and decoded key `lParam` bits.

Character contents, tokens, environment variables, command lines, and user-profile paths are never written. Existing evidence files are never replaced. Closing the target closes the test-owned sentinel and optional occluder; the target UI thread also closes its capture-evidence backdrop.

Prefer the repository-level `scripts/test-windows-computer-use.ps1` runner, which first binds canonical server/helper filenames and versions to one independently hashed frozen-candidate manifest, then launches them inside a private kill-on-close Windows Job Object, drives the authenticated loopback API, captures exact-target screenshots, checks independent non-interruption probes, and performs descendant/share/listener cleanup even after a failed assertion. See [Development](../../../docs/DEVELOPMENT.md#deterministic-windows-live-acceptance).
