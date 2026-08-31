# Withdrawn Windows candidate: trusted Git path was not scalar

This directory preserves the sanitized terminal record from the sole Windows
trust invocation for workflow run `33350856503`, attempt `7`. It is not release
evidence.

The frozen v0.12.66 candidate was bound to source commit
`2dcc8eccebaed6837a3eb71012e4826d7e9de922`, release-candidate artifact
`9754002931`, and raw artifact ZIP SHA-256
`c68b962eeae02f3a503e2c1bea67b994966a6e654ce9b2e88f681c56c4765dff`.
The API reported `16140761` artifact ZIP bytes. The fresh macOS aggregate
passed with SHA-256
`5bcb2b52d748a334a516b71dae8d55c11349090a76213b812487def169645217`,
but it cannot be reused or combined with a later workflow attempt.

The Windows launcher invoked the checked-in trust wrapper exactly once from a
local file that matched the exact source blob. Its Git discovery expression
requested the `Source` property from the application command resolver without
requiring a singular result. This environment returned two Git applications:
the system installation and a bundled tool installation. The resulting
`Object[]` was passed to the wrapper's scalar `TrustedGit` parameter and became
one combined string containing two drive-qualified paths. The second drive
separator made that combined value an invalid Windows path.

The clean Windows PowerShell child rejected the value while
`Assert-OrdinaryAbsolutePath` called `System.IO.Path.GetFullPath`. The wrapper
returned exit code 1. GitHub CLI discovery returned exactly one application;
the wrapper path and fresh short destination were ordinary scalar paths. No
absolute local path, username, credential, command line, or raw tool output is
retained here.

The failure preceded trust success, the fresh detached source clone, candidate
artifact download, candidate-byte execution, a Windows acceptance reservation,
runner launch, Computer Use initialization, UI action, and stock-Chrome
acceptance. Tagging, publication, and GitHub Release creation did not occur.

A future invocation must enumerate all returned Git applications, explicitly
select exactly one trusted system installation, and verify that the selected
value is a scalar rooted path naming an existing `git.exe` before passing it to
the trust wrapper. It must apply the same singular-selection check to GitHub
CLI. Literal surrounding quotes must not be stored in either path value.

The one-shot protocol makes this incomplete trust invocation terminal for
workflow attempt 7. It was not retried. These facts cannot be promoted,
combined with the successful macOS aggregate, or relabeled as acceptance for a
later attempt.
