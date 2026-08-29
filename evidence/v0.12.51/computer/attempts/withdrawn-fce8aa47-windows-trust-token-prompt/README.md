# Withdrawn Windows candidate: trust launcher omitted GH_TOKEN

This directory preserves the sanitized terminal record from the sole Windows
trust invocation for the v0.12.51 candidate. It is not release evidence.

The frozen candidate was bound to source commit
`2c4ce50f403e48683955e3d3a71139a072c860b8`, workflow run `33227697546`,
attempt `1`, release-candidate artifact `9707553849`, and raw artifact ZIP
SHA-256 `fce8aa47412dbbca8d605efc0c2962cc950a47d328b0f007b5981e0e1e30e2d7`.

The clean preflight found no v0.12.51 native reservation, coordinator, browser
ledger, candidate process, or listener on port 17373. The launcher then invoked
the checked-in trust wrapper exactly once, but did not place an authenticated
GitHub token in the process environment. The v0.12.51 wrapper completed its
fresh exact-source clone and blocked at its secure token prompt. Its retained
trust payload directory remained empty. No candidate artifact was downloaded,
no candidate executable was launched, Chrome and Computer Use were not opened,
and no UI action occurred.

The exact trust process tree was terminated during failure cleanup. The final
audit found no v0.12.51 candidate process or port 17373 listener. Absolute user
paths, process identifiers, credentials, command lines, and raw tool output are
not retained here.

The one-shot protocol makes the incomplete trust invocation terminal for this
exact candidate. It was not retried. Native helper acceptance, stock-Chrome
acceptance, evidence finalization, tagging, publication, and GitHub Release
creation did not occur. This record cannot be promoted, combined with another
attempt, or reused by a later version.
