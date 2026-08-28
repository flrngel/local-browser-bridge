# Withdrawn macOS candidate: finalizer received an empty output path

This directory preserves the sanitized dual-lane diagnostic record from the
sole v0.12.47 candidate. It is not release evidence.

The frozen candidate was bound to source commit
`8f611771b315bdb6ae34aa5a19e22d2c2ae95c13`, workflow run `33217699031`,
attempt `1`, release-candidate artifact `9704193584`, and raw artifact ZIP
SHA-256 `6f469e4e0f28c754c292f3a7a5e64671daba45ca3eed8212a1f1558b9d8ee64a`.
The canonical checksum manifest SHA-256 was
`9e6aae0f414a5916332fb59a6a192be9618ee6147eed376099c8dd1d99fc9a30`,
and the macOS archive SHA-256 was
`6670b301d6f427ad18342bea76a6cc754574dcfa2cb15812a65724edaac83ec7`.

Both fresh packaged macOS lanes passed and their twelve screenshots passed
mandatory visual review. The quiet lane completed 207/207 checks with result
SHA-256 `2b638da2ec177f803e8726fb37c1f5ae344ed175207d75426bf36958abe7726d`.
The separately authorized exact-app-share lane completed 231/231 checks with
result SHA-256
`53b77a249151c6136dbdb9cbcaa49170b1cef1e62b059052d8943f94693bb1b8`.
The exact `lbb-app-share-start` button was pressed once, its receipt was
observed, the product action completed with quiet shared-seat boundaries, and
the Computer Use session was reset immediately afterward.

The candidate was nevertheless withdrawn because the coordinator combined two
documented sequential shell assignments into one `export` command under
`set -u`. The shell expanded `AGGREGATE_CANONICAL` before `AGGREGATE_DIR` was
available, emitted `AGGREGATE_DIR: parameter not set`, and invoked the
finalizer with an empty output argument. The finalizer correctly returned
`aggregate output directory path is invalid.` No aggregate was created; the
fresh owner-private aggregate directory remained empty.

The acceptance protocol makes any finalizer nonzero terminal for that exact
candidate, including coordinator mistakes. The finalizer was not retried.
Windows native acceptance, stock Chrome acceptance, evidence finalization,
tagging, publication, and GitHub Release creation did not start. The retained
nineteen lane files are sanitized harness output only and cannot be promoted,
combined with another attempt, or reused by a later version.
