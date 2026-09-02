# Local Browser Bridge project rules

- The server must run as a compiled Rust binary and must not require Node.js at runtime.
- All repository text, documentation, UI copy, logs, and user-facing output must be written in English only.
- Keep the Rust package version and Chromium extension manifest version aligned. Bump both for every completed extension/package work item.
- In this repository, the user command `deploy` means all of the following:
  1. Commit and push the intended release with a Conventional Commits message.
  2. Build a Windows x86_64 `.exe` and macOS universal binary that run without Node.js, and package the Chromium extension as the same versioned ZIP (`deploy.yml`).
  3. Let acceptance verify the packaged candidate before publishing it — see [docs/maintainers/RELEASE.md](docs/maintainers/RELEASE.md) for the current procedure.
  4. Publish the binaries, extension ZIP, SHA-256 manifest, and GitHub provenance in an immutable public GitHub Release (`publish.yml`).
  5. Download every published asset into `dist/` and verify checksums, file formats, architectures, archive contents, and attestations.
  6. Return clickable local artifact paths and the public release link in the final response.
- Commit completed work with a Conventional Commits message.
