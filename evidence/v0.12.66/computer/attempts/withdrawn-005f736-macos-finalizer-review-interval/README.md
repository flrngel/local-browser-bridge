# Withdrawn macOS candidate: final review interval expired

This directory preserves a sanitized record of the sole macOS acceptance
invocation for workflow run `33350856503`, attempt `8`. It is not release
evidence.

The frozen v0.12.66 candidate was bound to source commit
`2dcc8eccebaed6837a3eb71012e4826d7e9de922`, release-candidate artifact
`9754833773`, and raw artifact ZIP SHA-256
`005f73614c7f42ef98a55e30bce25d86e46d2d92552c70bac21aaa52838ff815`.
The API reported `16140697` artifact ZIP bytes.

Both fresh macOS product lanes completed successfully. The quiet lane passed
207 of 207 checks with result SHA-256
`00f0f699d57c0ae4f4be1a52905001c6fde318dd2f57e098f609826c3de92069`.
The deliberate-concurrency lane passed 231 of 231 checks with result SHA-256
`ba0901100d2bcc1bde68e2f9e1ee3cc47de88e8619af5d0fab27bf0a89cc13f9`.
All twelve screenshots were visually reviewed against the exact-window,
version, lane, counter, persistent-stream, click, resize, crop, unrelated-pixel,
and sensitive-pixel criteria. The exact app-share handoff received one
authorized semantic click and completed.

The create-once aggregate finalizer then rejected the evidence because Phase 3
was not completed within the bounded 30-minute interval measured from the
deliberate result's `capturedAt` value. No aggregate
`macos-acceptance.json` was created. The bound is an acceptance invariant and
was not weakened, bypassed, or relabeled.

The delay occurred while recovering the original GNU Screen writer after a
secondary attachment could observe the shell but could not reliably deliver
the reviewed digest. Product commands were not retried. No candidate evidence
was copied into the repository, and Windows, stock-Chrome, tagging,
publication, and GitHub Release creation did not start.

The one-shot protocol makes this incomplete finalization terminal for workflow
attempt 8. The candidate artifact must be withdrawn. These lane results cannot
be promoted, combined with a later workflow attempt, or relabeled as release
acceptance. A future attempt must use a fresh candidate, fresh private parent,
fresh lane results, a fresh exact-app-share action, and complete visual review
and create-once finalization inside the bounded interval.
