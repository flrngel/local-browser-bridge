# Local Browser Bridge project rules

- The server must run as a compiled Rust binary and must not require Node.js at runtime.
- Keep the Rust package version and Chromium extension manifest version aligned. Bump both for every completed extension/package work item.
- In this repository, the user command `deploy` means all of the following:
  1. Build a Windows x86_64 `.exe` that runs without Node.js.
  2. Package the Chromium extension directory as a versioned ZIP.
  3. Generate checksums and place all release artifacts in `dist/`.
  4. Verify the binary and archive contents.
  5. Return clickable paths to the artifacts in the final response.
- Commit completed work with a Conventional Commits message.
