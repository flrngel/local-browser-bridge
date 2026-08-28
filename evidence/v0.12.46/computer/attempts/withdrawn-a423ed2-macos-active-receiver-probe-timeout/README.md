# Withdrawn v0.12.46 macOS active-receiver probe timeout

The sole v0.12.46 release candidate was built by workflow run `33215024614`,
attempt `1`, artifact `9703239886`, from source
`5b39b8d2a7ea216175b8936670392c1e22750d77`. Its raw artifact ZIP SHA-256 was
`a423ed25f9b6fb2b0db8f3065c55f0a7309a4c4ff0f88fc8a5c413047dabacde`, and
its canonical checksum-manifest SHA-256 was
`acb27bd2330aa0940f338a5692c98a2972e9e6e9fabe7d16d127ad41bd2bfe8f`.

The candidate passed source, artifact, provenance, package, architecture,
signature, permission, quiet-seat, helper-readiness, exact-window discovery,
semantic-action, persistent-share, and same-PID sibling-receiver gates. The
quiet lane reached 68 of 69 assertions, then failed closed before product
dispatch because the independent SystemProbe did not observe the primary
window as the active AX main/focused receiver inside its eight-second wait.
The bounded failure record proves that the user's foreground and AX focus,
cursor, Space, HID counters, and prior sibling receiver remained unchanged.
The fixture click count stayed zero and `actionDispatched` stayed false.

The product command remained eligible for the server's fifteen-second call
boundary, but the external receiver watcher requested only eight seconds and
the native probe capped any request at ten seconds. Therefore this result does
not prove a product dispatch failure; it proves that the acceptance watcher
could terminate before the command boundary. v0.12.47 binds both sides to one
sixteen-second bounded watcher deadline, one second beyond the server call
boundary, and enforces that relationship in the source contract tests.

The four retained PNGs were visually reviewed. They contain only the exact
quiet-lane fixture and cover initial observation, semantic value, semantic
invoke, and persistent-share start. The machine result and log are the
runner-sanitized create-once files. No deliberate macOS lane, Windows lane,
stock-Chrome lane, evidence aggregate, tag, publication workflow, or GitHub
Release followed. This candidate is terminal and must never be retried or used
as passing evidence.
