# Withdrawn v0.12.42 macOS package-inventory attempt

This directory preserves the sanitized terminal record for the pre-execution
macOS acceptance attempt bound to source
`014800df829b0c1456dd5ae775a51a941c5afc40`, candidate workflow run
`33117043972` attempt `1`, and final artifact `9665341270`.

The checked-in candidate trust gate passed. It independently verified the
16,137,066-byte raw artifact ZIP with SHA-256
`dcb978c8a6bd930c356aedab68d7f50606d3b931e330840e567fff44d32504b5`,
the checksum-manifest SHA-256
`3e98e00f476b35fd40cb6bc8042c71050a5c56c6e905eef187f5053503c57647`,
the exact five-file payload, every asset digest, and all five exact-attempt
GitHub attestations from GitHub-hosted runners.

The bounded package preparer then failed closed because the release archive
contained the newly shipped `Local Browser Bridge.app` desktop-host bundle
while the v0.12.42 acceptance inventory still described the earlier archive
without that bundle. The first rejected member was the top-level
`Local Browser Bridge.app` directory. The archive itself had already passed the
release asset verifier and was not malformed; the acceptance allowlist was
stale relative to the intentional package layout.

This failure occurred before quiet-seat readiness, permission probes, package
extraction completion, or any candidate executable invocation. No candidate
server, desktop host, helper, listener, fixture, screenshot, app-share action,
Windows acceptance, stock-Chrome acceptance, tag, or GitHub Release existed.
The preparer removed its partial output. The exact candidate was not retried
and is withdrawn.
