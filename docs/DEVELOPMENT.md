# Development

No Node.js runtime is required. The entry points are Rust executables; the macOS build links a bundled Swift ScreenCaptureKit bridge and system frameworks. Some checked-in browser evidence rigs use JavaScript tooling, but they are optional evidence harnesses rather than production dependencies.

## Prerequisites

- Rust 1.88 or later with Cargo and rustfmt
- Platform SDK and linker for the target operating system
- `zip` and `unzip` for extension packaging
- Chrome or Edge 118+ for core browser testing
- Chrome or Edge 125+ for recursive cross-origin iframe testing
- macOS 13+ with Screen Recording and Accessibility permissions for native macOS testing
- A signed-in interactive Windows 11 session for native Windows testing

Use the exact versions in `Cargo.lock`. Distributed versions in `Cargo.toml`, `Cargo.lock`, `extension/manifest.json`, and `extension/lib.js` must remain aligned.

## Repository layout

| Path | Purpose |
|---|---|
| `src/` | Rust server, protocol, update check, and computer helper code |
| `extension/` | Manifest V3 Chromium extension |
| `public/` | Control page and local demo embedded into the server |
| `tests/` | Rust contract and integration tests |
| `evidence/` | Versioned live-run results and screenshots |
| `packaging/` | Platform packaging metadata |
| `scripts/` | Version, package, deploy, and artifact verification scripts |
| `docs/` | Protocol, architecture, research, capability, and limitation references |

## Build

```bash
cargo build --locked --release
```

The two main binaries are:

```text
target/release/local-browser-bridge
target/release/local-computer-helper
```

The server embeds its UI. Do not add a runtime dependency on a JavaScript package manager, remote script, or CDN.

## Local protocol test

Start the server:

```bash
cargo run --locked --release
```

In a second terminal, run the Rust mock connector:

```bash
cargo run --locked --release --bin mock-extension
```

Open the complete authenticated control-page URL printed by the server. The server and mock connector read the same generated token file unless `LBB_TOKEN` or `LBB_TOKEN_PATH` overrides it.

### Runtime configuration

| Variable | Default | Purpose |
|---|---|---|
| `LBB_PORT` | `17373` | Loopback HTTP/WebSocket port |
| `LBB_TOKEN` | Generated automatically | Explicit bridge token |
| `LBB_TOKEN_PATH` | Platform user-data path | Generated-token storage path |
| `LBB_DISABLE_UPDATE_CHECK` | `false` | Disable the one-time GitHub metadata check |

The equivalent `--no-update-check` flag disables the startup check; `--check-updates` performs the metadata-only check and exits. The server still binds only to loopback.

## Test the real extension

1. Open `chrome://extensions` or `edge://extensions`.
2. Enable Developer mode.
3. Select **Load unpacked** and choose `extension/`.
4. Enter the server token and port in the extension popup.
5. Use `/demo` for ordinary element, key, scroll, dialog, and navigation checks.
6. Confirm the Chrome-owned debugger warning, page pill, trusted Stop, revocation, and reconnect behavior.

Use Chromium 125 or later for the nested cross-origin fixtures. A passing same-process iframe test does not prove the OOPIF route.

## Test the computer helper

Run the helper in a third terminal:

```bash
cargo run --locked --release --bin local-computer-helper
```

On macOS, request permissions through the packaged app identity when testing release behavior. A raw `cargo run` process is useful for development but is not evidence for the packaged TCC flow.

Test one-shot observation and persistent sharing separately:

- `computer.observe` exercises the exact-window snapshot path.
- `computer.share.start` exercises ScreenCaptureKit `SCStream` on macOS or Windows Graphics Capture on Windows.
- Share tests must cover start, first useful frame, monotonic sequences, bounded replacement, dropped-frame accounting, target closure, explicit stop, and connector replacement.
- Input tests must prove an application-owned result plus unchanged platform-specific foreground/window-focus oracle, hardware pointer, and active desktop. The before/after samples do not prove the absence of an unobserved transient change.

Do not report cross-Space, minimized, protected, elevated, or framework-specific behavior as supported merely because a stream or message API returned success.

## Required checks

```bash
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
bash scripts/audit-versions.sh
```

For documentation changes, also run:

```bash
git diff --check
rg -nP '\p{Hangul}' README.md SECURITY.md docs extension public src tests
```

The second command must return no matches because all repository and user-facing text is English.

## Extension package

Create the deterministic, allowlisted extension ZIP with:

```bash
bash scripts/package-extension.sh
```

The script rejects missing, linked, or unexpected package files and verifies that archive contents match `extension/` byte for byte.

## Evidence discipline

Plan verification before changing a capability. Keep these evidence classes separate:

1. Unit or contract proof: validates local logic and failure boundaries.
2. Integration proof: validates connector/server behavior.
3. Live application proof: validates the real browser or OS accepted the action.
4. Packaged release proof: validates the exact immutable artifacts users download.

Record negative results. A candidate run must not be described as published release evidence. Screenshots should show the relevant browser or OS indicator, target result, and non-interruption state without exposing tokens, personal data, or authenticated URLs.

See [SOTA audit](SOTA_AUDIT.md) and the [evidence index](../evidence/) for current boundaries.

## Versioning

Every completed extension/package work item or deployment must bump and align:

- the Rust package version;
- `Cargo.lock`;
- the extension manifest version; and
- the extension library version.

Run `bash scripts/audit-versions.sh` before packaging. Commit finished work with a Conventional Commits message unless the current task explicitly says not to commit.

## Deployment

In this repository, `deploy` means more than a local build. It means committing and pushing the intended version, building the Windows server and helper, building the universal macOS server/helper archive, packaging the matching extension, publishing all artifacts plus `SHA256SUMS.txt` and GitHub provenance in an immutable public GitHub Release, then downloading and verifying every published asset.

The canonical release path is `.github/workflows/deploy.yml` from a matching `vVERSION` tag. On a suitably configured macOS host, `scripts/deploy.sh` performs the local cross-platform build; Windows cross-compilation requires `cargo-xwin` or a supported MinGW fallback.

Verify a downloaded release set with:

```bash
bash scripts/verify-release-assets.sh VERSION dist
```

See [Installation](INSTALL.md) for the user-facing artifact verification flow and [Architecture](ARCHITECTURE.md) for component boundaries.
