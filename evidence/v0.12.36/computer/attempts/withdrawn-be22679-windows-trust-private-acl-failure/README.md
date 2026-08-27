# Withdrawn v0.12.36 Windows trust-destination ACL attempt

This directory preserves the sanitized terminal record for the Windows
acceptance task bound to source
`be22679f68f953032a39b77987f48af5e4f277c1`, candidate workflow run
`33023578777` attempt `1`, and final artifact `9627794815`.

The checked-in Windows release-candidate verifier was invoked exactly once. It
created its fresh destination directory, then exited with code 1 while applying
the required owner-private ACL. Read-only post-failure inspection found an
ordinary empty directory whose owner matched the current principal, but whose
access rules were still inherited and unprotected. Five inherited allow rules
remained. The verifier creates the destination and applies that ACL before it
creates any trust subdirectory, clones source, reads GitHub artifact metadata,
or downloads candidate bytes, so the empty unchanged directory localizes the
failure to `set-private-destination-acl`.

The outer capture retained the verifier exit code but did not persist or print
the private error stream. The exact inner exception and native error code are
therefore unavailable and are not claimed. That diagnostic limitation is
fail-closed: the verifier was not rerun and no candidate was relabeled.

No source clone, artifact ZIP, payload, candidate binding, manifest result, or
attestation result was produced. The Windows acceptance coordinator was not
started, the durable v0.12.36 reservation remained absent, and no candidate
server, helper, fixture, listener, foreground handoff, Computer Use action, or
Chrome action occurred.

Cleanup removed only the exact empty task-owned trust destination after
confirming its identity and absence of reparse points. No relevant process or
listener remained. This committed inventory contains only reduced, sanitized
facts. It excludes local paths, usernames, process identifiers, command lines,
credentials, tokens, environment identifiers, candidate bytes, and raw logs.
