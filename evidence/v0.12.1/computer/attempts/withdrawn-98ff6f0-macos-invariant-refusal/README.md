# Withdrawn macOS candidate: fail-closed invariant refusal

This directory preserves the sanitized negative result from candidate commit
`98ff6f06a0da12602d00189640a188b940967ae6`. It is not release evidence.

The packaged archive matched SHA-256
`57906096dd0f3b2046ac55140254d9165b0e7fe7b6863b4688a9693bb5bc5994`.
The run passed 48 checks through exact-window observation, semantic value
confirmation, semantic invocation, and persistent ScreenCaptureKit startup.
It then refused a background pixel action with HTTP 504
`COMPUTER_OUTCOME_UNKNOWN` after a non-interruption invariant changed. The
candidate's earlier error message did not identify the action stage or the
individual failed invariant, so the result cannot establish whether external
session activity or product input delivery caused the change.

The four retained screenshots contain only the deterministic fixture window.
`helper-results.json` and `helper-rig.log` contain the complete sanitized
machine-readable result. The candidate was withdrawn rather than retried or
treated as a pass; it was also independently invalidated by cross-platform CI.
