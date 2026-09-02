# Evidence

This directory is **frozen history**. Each `vX.Y.Z/` folder records what a
past release-acceptance attempt actually observed — screenshots, machine
results, and (for withdrawn attempts) a sanitized negative record — for the
version named by its folder.

Do not delete or rewrite the contents of an existing version folder; treat
each one as an immutable record, the same way a merged pull request's diff is
immutable. Do not add new version folders here either: **new acceptance
evidence lives in CI workflow artifacts**, not in this repository, now that
release acceptance runs in CI (see [Release process](../docs/maintainers/RELEASE.md)).
This directory stops growing at the last version verified by the prior
operator harness.

For the release-by-release story of what was attempted, what failed, and
why — the narrative that used to be duplicated across several docs pages —
see [Release-attempt history](../docs/history/release-attempts.md). For the
current capability and limitation matrices, see
[Capabilities](../docs/CAPABILITIES.md) and
[Limitations](../docs/LIMITATIONS.md).
