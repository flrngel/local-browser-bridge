# Local patch policy

This directory vendors the published `screencapturekit` crate version `8.0.1`
from upstream commit `2a9f13bcbeadb0aabc5596f0ff3d2ba71da8c1d0` under its existing MIT or
Apache-2.0 license. The original crates.io archive checksum is
`9ddaa8d6b16a2762c9a97c9a6297f04cb8ded0487e5ef02dc98b4e2bee3a26c7`.

Local Browser Bridge carries one runtime-availability correction in
`swift-bridge/Sources/ScreenCaptureKitBridge/Stream.swift`:

- `SCStream.updateConfiguration` is guarded at macOS 13.0, matching this
  package's deployment floor, instead of being incorrectly refused before
  macOS 14. Apple exposes the underlying API from macOS 12.3.

The patch keeps the public Rust API and crate version unchanged. Remove the
path override after an audited upstream release includes an equivalent fix.

Authoritative API reference:
<https://developer.apple.com/documentation/screencapturekit/scstream/updateconfiguration%28_%3Acompletionhandler%3A%29>
