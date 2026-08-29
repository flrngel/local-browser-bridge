# Withdrawn macOS candidate: coordinator supplied the wrong scratch path

This directory preserves the sanitized macOS diagnostic record from the sole
v0.12.50 candidate. It is not release evidence.

The frozen candidate was bound to source commit
`ff549a5e4fd35c7fad299a7f3733b8de4e6c3194`, workflow run `33226239420`,
attempt `1`, release-candidate artifact `9707082268`, and raw artifact ZIP
SHA-256 `e5530cea34ef89733dbb4088baca019fe6c0683621f945f12d962ef0bdf6a407`.
The canonical checksum manifest SHA-256 was
`2ebc5894ab4fe66c74e7c19d6b3fc5ef7bf89dce529217bc44d4388caffd3d81`,
and the macOS archive SHA-256 was
`6de6e357f09667b97eba52ce22d376c13f3daf45cab44fbd0072f2b8199221de`.

The fresh packaged quiet lane passed 207/207 checks with result SHA-256
`a5428392121903ab3fad2c87f80674232a332444b0b9217d139c92092316d3a0`.
All six screenshots passed mandatory visual review: the primary fixture alone
showed the quiet lane, exact v0.12.50 semantic value, completed semantic action,
persistent stream, one background pixel click, and settled 820x520 resize.

The candidate was nevertheless withdrawn before any deliberate-lane candidate
byte executed. The coordinator supplied a nonexistent owner-private scratch
parent to the deliberate runner. After seven source and output-boundary checks,
the runner correctly stopped with `ENOENT`. The retained sanitized failing
result SHA-256 is
`8d3e5a37ced360fc148a60556f0ebe9b7e7a1c631b342e69de76ee6c0efe1447`.
The original absolute temporary path was replaced with the explicit
`<redacted-owner-private-scratch-parent>` placeholder in the retained result
and log.

The acceptance protocol makes any runner nonzero terminal for that exact
candidate, including coordinator mistakes. The deliberate lane was not
retried, no app-share UI action occurred, and no macOS aggregate was created.
The remote Windows and stock Chrome acceptance task was immediately told to
stop. Evidence finalization, tagging, publication, and GitHub Release creation
did not occur. These retained files cannot be promoted, combined with another
attempt, or reused by a later version.
