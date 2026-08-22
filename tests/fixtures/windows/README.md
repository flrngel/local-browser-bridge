# Windows computer-use fixture

`WindowsComputerUseFixture.ps1` is a deterministic, fixture-owned target for live Windows helper acceptance. It includes a topmost `#101820` backing mat beyond the target so the acceptance runner can retain a narrowly cropped desktop-level capture-indicator proof without retaining unrelated desktop pixels. It must be launched with an explicit, new evidence directory:

```powershell
.\tests\fixtures\windows\WindowsComputerUseFixture.ps1 -EvidenceDirectory $temporaryEvidence
```

Use `-ShowOccluder` to place a no-activate magenta window over the animated target. An exact-window capture must not include those magenta pixels.

The fixture writes only these files beneath the caller-selected directory:

- `fixture-ready.json`: process and fixture-owned HWND identifiers;
- `fixture-state.json`: current counters, bounds, foreground/cursor probes, and SHA-256/length text proofs;
- `fixture-events.ndjson`: fixture-owned input events, non-character message parameters, redacted character messages, and decoded key `lParam` bits.

Character contents, tokens, environment variables, command lines, and user-profile paths are never written. Existing evidence files are never replaced. Closing the target closes the test-owned sentinel and optional occluder; the target UI thread also closes its capture-evidence backdrop.

Prefer the repository-level `scripts/test-windows-computer-use.ps1` runner, which launches the matching server and helper inside a private kill-on-close Windows Job Object, drives the authenticated loopback API, captures exact-target screenshots, checks independent non-interruption probes, and performs descendant/share/listener cleanup even after a failed assertion. See [Development](../../../docs/DEVELOPMENT.md#deterministic-windows-live-acceptance).
