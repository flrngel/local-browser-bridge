# Withdrawn v0.12.9 macOS exact-candidate attempt

This directory preserves the failed macOS acceptance run for the exact v0.12.9
release candidate. The protected publication job was canceled without approval,
Windows and stock-Chrome acceptance were not started, and no v0.12.9 GitHub
Release was created. Nothing in this directory is evidence for a shipped
release.

## Frozen candidate binding

- Source commit: `db624dac051df32c779257d8a13f6e0e3a33bbad`
- Annotated tag object: `b7b909412f4ff17508b63d5b440edf5703591053`
- Tag: `v0.12.9`
- Deploy workflow run: `32631182705`, attempt `1`
- Release-candidate artifact: `9491269939`
- Artifact ZIP bytes: `10356338`
- Artifact ZIP SHA-256:
  `4ec0bafcc74631e145a54a567960a82643057c8dda0612c91d46d38929af66d3`
- `SHA256SUMS.txt` SHA-256:
  `56a87068ac150d322edf923696bd93f4c9ef0f10dd752ef48bfd7ca0e1950500`
- macOS archive SHA-256:
  `7959a18f58a5f96861b8647edbb8059222e3a5b47df45f52dfe5ac5174c43a8e`
- Packaged server SHA-256:
  `44d3b46c456b803e2801776ea730199a3f8072541d4df9f449b7432343387909`
- Packaged helper SHA-256:
  `67e7ce5e4cc45472a19107c8ec70f80984e069101df63bcf46b7905d5ce70801`

Before either executable ran, two independent coordinator checks verified the
artifact API binding, raw artifact digest, exact five-file inventory, canonical
four-line manifest, every payload hash, source/tag/workflow/run-attempt
identity, GitHub-hosted runner identity, and all five candidate-file GitHub
attestation checks. Both macOS executables were universal `arm64`/`x86_64`,
reported version 0.12.9, and passed strict structural code-signature checks.

## Result and withdrawal reason

`helper-results.json` is a schema-3 `failed-release-candidate` record. The rig
recorded 40 passing assertions and zero assertion failures before a fatal
post-dispatch refusal; that assertion count is not a passing result. It used
one exact packaged helper process, discovered the exact target and genuine
same-process sibling, observed the exact target, and selected its Accessibility
text field. The subsequent `computer.setValue` returned HTTP 504
`COMPUTER_OUTCOME_UNKNOWN` with
`COMPUTER_BACKGROUND_CONTRACT_VIOLATION` at `semanticSetValue` because the
sampled global hardware-cursor position differed across the action.

The retained record cannot attribute that cursor change to the helper rather
than concurrent physical user input. It therefore cannot prove the required
non-interruption invariant and correctly fails closed. The sibling remained
the target app's remembered focused and main window after the refusal. The run
stopped before semantic read-back, persistent sharing, pixel input, native text,
resize, cancellation, recovery, or target-close proof. The candidate was not
retried or reused.

This failure exposes an attribution gap in the invariant model: equality of two
global cursor samples cannot distinguish helper-caused motion from unrelated
physical motion. Any successor must preserve the ban on global HID/warp APIs
and report observed user motion truthfully without treating an unowned physical
event as proof that a target-routed semantic action moved the cursor.

## Retained inventory

| File | Bytes | SHA-256 |
|---|---:|---|
| `computer-01-exact-window-observe.png` | 779471 | `550816e6e8a77a3dcfea111e7393c2976b17484609d8e45fe3c170c5883e67ce` |
| `helper-results.json` | 8164 | `862184d1d8bce46d64c854a768dadef5efe27747abdb9972c726e5ea6eeb795e` |
| `helper-rig.log` | 4640 | `f435b55373a6bfb28a8d002002762b1e174f2af55c541bb1a42f9563ac9160f1` |

The screenshot record matches the retained PNG. Visual review confirmed that
it contains only the deterministic primary fixture: no sibling window,
desktop, unrelated application, personal content, native text payload, or
secret is visible. The PNG is 1209 by 826 RGBA and contains only `IHDR`, `IDAT`,
and `IEND` chunks, with no retained metadata chunk.

The result and log contain no bearer token, authorization value, user/home
path, hostname, email address, signed URL, command-line secret, or unrelated
content. After the fatal error the log records the owned helper, server, and
fixture stopping and the scratch directory being removed. A separate
current-state check found no related process or default loopback listener.
