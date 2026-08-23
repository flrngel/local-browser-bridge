# Withdrawn v0.12.6 Windows foreground-sentinel attempt

This directory preserves the failed interactive-Windows acceptance run for the
exact v0.12.6 release candidate. The candidate was withdrawn, its protected
publication job was canceled without approval, and no v0.12.6 GitHub Release
was created. Nothing in this directory is evidence for a shipped release.

## Frozen candidate binding

- Source commit: `397e4b6abac794141a028dc52b3216cbacc055b7`
- Annotated tag object: `7186326dcf10d01e6c213fc241d0940c1a036de3`
- Tag: `v0.12.6`
- Deploy workflow run: `32617378542`, attempt `1`
- Release-candidate artifact: `9487511503`
- Artifact ZIP SHA-256:
  `3cc22cc8958b6c201ac05f22fde95f225dc54b83c694b055f677c3083723580e`
- `SHA256SUMS.txt` SHA-256:
  `8c3759a53f10d5c9bb5fbae03bb3fe4bb84e39073647ee9d6c7ee2f42392f3c3`
- Windows server SHA-256:
  `72cef4966edb0ef18b33210cc8808f669e22d23ed3372a0190b1be2bf78be94e`
- Windows helper SHA-256:
  `e27d9b2e75a3897709c3121568469471bc0fa0be945122fa08f56a52e1d3c652`

Before execution, independent coordinator and Windows-host checks verified the
exact five-file artifact inventory, canonical four-line checksum manifest,
every payload hash, PE32+ x86-64 identity, clean detached source, annotated tag,
workflow/run-attempt identity, GitHub-hosted runner identity, and all five
GitHub attestations. A short-path checkout with Git long-path support contained
all 365 tracked files and passed `git fsck --full`. Windows PowerShell 5.1 then
passed the runner self-test.

## Result and withdrawal reason

The live run failed closed after its first read-only `computer.status` step and
before observation, capture, sharing, or input. The packaged server and helper
connected through the expected protocol session; the initial disposable worker
matched the exact image, parent, and interactive session. The runner then timed
out waiting for the fixture-owned foreground sentinel. No screenshot was
created.

The retained fixture state proves that its sentinel was shown and received one
WinForms activation callback, but the independent `GetForegroundWindow` oracle
did not match the sentinel. The v0.12.6 fixture treated `Shown` as readiness and
made only one unchecked `Form.Activate()` attempt. Windows may deny that request
when another application owns the foreground; `TopMost` and a WinForms
activation callback are not global-foreground proof. This is an acceptance
setup defect, not a computer-action failure, because no product mutation ran.

Version 0.12.7 replaces implicit activation with a post-readiness, test-owned
click-to-arm handshake. Both a fresh left-mouse down and its matching up must
occur for the exact request generation while native foreground and focus match
the sentinel and its exact button; focus loss, deactivation, or a new request
clears the pending press. The runner then requires three consecutive stable
native foreground, focus, cursor, and input-desktop samples, each bound to a
distinct advancing fixture-state publication, followed by another fresh
baseline publication. It then re-binds the original authenticated helper
session and PID before the first baseline command; a stale state writer or
worker restart fails closed.

It does not use `SetForegroundWindow`, `AttachThreadInput`, `SendInput`, an Alt
key workaround, or an automatic focus-stealing loop. The receipt assumes a
trusted interactive acceptance session and does not claim cryptographic proof
that a physical human, rather than an authorized Computer Use surface, produced
the window messages.

## Retained inventory

| File | Bytes | SHA-256 |
|---|---:|---|
| `fixture/fixture-events.ndjson` | 877 | `7a9d20f90c1ae7ec0badc7c28e40a0946b21622f79b92be05f6f99d67ee60a70` |
| `fixture/fixture-ready.json` | 181 | `800549e7281115d79cc933c98b95d61360e0e6dc6582fee93b22e3e737b389c1` |
| `fixture/fixture-state.json` | 1015 | `f7f71e78deaf8854070d94d0afc8b1d1b9e61fe641646bdd5b9442094e8bee7c` |
| `steps/01-protocol-bound-helper-readiness.json` | 7864 | `2719e172911aa00201b5b257e7c51a52279ab76b24e8aea9b2e3bc223d1ae84f` |
| `summary.json` | 9008 | `c44743bdb1b805aacedf1faa555729bd0f8c36c4fdc96e46e4d0fd4cc88ab4ff` |

The copied records preserve the exact retained bytes from the Windows host. A
separate leakage scan found no bearer token, authorization value, credential,
email address, user or home path, signed URL, environment identifier, or
unrelated title.

Cleanup completed with no issues. Token-persistence verification passed, no
token-bearing evidence was removed, the runner terminated no unrelated
process, the one-shot recovery event was released, and an independent check
found no candidate server/helper process or relevant loopback listener.
