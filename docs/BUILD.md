# Building from source

This guide builds the current source tree. A local build is a development
artifact: it is not a published GitHub Release, has no release attestation, and
has not passed the repository's packaged acceptance gates. End users who want a
stable, verified package should use the [installation guide](INSTALL.md).

The application runtime is Rust-only. Node.js is never invoked by the server,
helper, or extension. Node.js 24 is required only when running the complete
developer contract suite.

## Build matrix

| Target | Build host | Result |
|---|---|---|
| Windows x86_64 | 64-bit Windows with MSVC and Windows SDK | Server and helper `.exe` files |
| macOS host architecture | macOS with SDK 26+ | Raw development server and helper binaries |
| macOS universal | macOS with SDK 26+ | Universal server and `Local Computer Helper.app` archive |
| Chromium extension | Any host for direct unpacked use; Bash plus `zip`/`unzip` for the deterministic ZIP | Manifest V3 unpacked directory or ZIP |

The project distributes Windows and macOS packages. A successful compile on an
unlisted host does not make its computer helper a supported platform.

## Common requirements

For a native compile:

- Git
- `rustup`, Cargo, and Rust 1.88 or later
- the target platform's native SDK and linker

Install the minimum toolchain without changing the machine-wide default:

```text
rustup toolchain install 1.88.0 --profile minimal --component rustfmt,clippy
```

All commands below use `cargo +1.88.0` so they exercise the declared minimum.
A newer stable toolchain is also supported.

Additional tools are needed for the complete repository checks:

- Node.js 24 for extension, dashboard, and evidence-harness behavior tests
- Bash and Python 3 for Unix-only contract helpers
- `zip` and `unzip` for deterministic extension packaging
- `cargo-about 0.9.2` with its `cli` feature for dependency-license checks
- Chrome or Edge at or above the current extension's declared minimum for live
  browser testing

None of those test-only tools is packaged into the product.

## Windows build

### 1. Install the Windows toolchain

Use 64-bit Windows 11 and install:

- Git for Windows;
- Visual Studio 2022 or Visual Studio 2022 Build Tools;
- the **Desktop development with C++** workload, including MSVC v143 x64/x86
  build tools; and
- a Windows 11 SDK.

The SDK provides `mt.exe`; the Visual C++ tools provide `dumpbin.exe`. The
repository's artifact verifier uses both. The Windows target embeds an
`asInvoker`, `uiAccess=false`, Per-Monitor-V2 manifest and uses the static C
runtime, so a separate Visual C++ Redistributable is not part of the package.

Install Rust's MSVC target:

```powershell
rustup toolchain install 1.88.0-x86_64-pc-windows-msvc `
  --profile minimal --component rustfmt,clippy
rustup target add --toolchain 1.88.0-x86_64-pc-windows-msvc `
  x86_64-pc-windows-msvc
```

### 2. Clone into a short, long-path-enabled checkout

The versioned evidence inventory contains tracked paths that exceed the legacy
Windows path limit. A normal deep checkout can silently omit them and leave Git
reporting tracked deletions. Use a short root and enable long paths for the clone
itself:

```powershell
New-Item -ItemType Directory -Force C:\src | Out-Null
git -c core.longpaths=true clone `
  https://github.com/flrngel/local-browser-bridge.git `
  C:\src\local-browser-bridge
Set-Location C:\src\local-browser-bridge
git config core.longpaths true
```

Confirm that the checkout is complete before building:

```powershell
git status --porcelain=v2 --untracked-files=all
git ls-files --deleted
```

Both commands must produce no output.

### 3. Build the two product executables

```powershell
cargo +1.88.0-x86_64-pc-windows-msvc build --locked --release `
  --target x86_64-pc-windows-msvc `
  --bin local-browser-bridge `
  --bin local-computer-helper
```

Outputs:

```text
target/x86_64-pc-windows-msvc/release/local-browser-bridge.exe
target/x86_64-pc-windows-msvc/release/local-computer-helper.exe
```

The `mock-extension` binary is development tooling and is not a release asset.

### 4. Verify the development executables

```powershell
$version = (Select-String -Path Cargo.toml `
  -Pattern '^version = "([^"]+)"$').Matches[0].Groups[1].Value
$server = "target/x86_64-pc-windows-msvc/release/local-browser-bridge.exe"
$helper = "target/x86_64-pc-windows-msvc/release/local-computer-helper.exe"

& .\scripts\verify-windows-artifacts.ps1 `
  -Version $version `
  -ServerPath $server `
  -HelperPath $helper
```

The verifier checks the PE architecture, embedded manifest, VERSIONINFO,
static-runtime imports, helper input API surface, reported version, and embedded
licenses. It executes only the just-built local files with bounded informational
flags.

### 5. Run the development build

First PowerShell window:

```powershell
& .\target\x86_64-pc-windows-msvc\release\local-browser-bridge.exe
```

Second PowerShell window, only when desktop control is needed:

```powershell
& .\target\x86_64-pc-windows-msvc\release\local-computer-helper.exe
```

Load the repository's `extension` directory directly from
`chrome://extensions` or `edge://extensions`. No extension build step is needed
for local development.

### 6. Package the extension on Windows

The canonical packaging script requires Bash, `zip`, and `unzip`. Run it from a
compatible Git Bash, MSYS2, or WSL environment whose checkout has the same exact
source bytes:

```bash
bash scripts/package-extension.sh
```

Output:

```text
dist/local-browser-bridge-extension-vVERSION.zip
```

If those tools are not installed, use the unpacked `extension` directory for
development instead of creating a noncanonical ZIP with another archiver.

## macOS build

### 1. Install the macOS toolchain

The resulting package runs on macOS 13 or later, but the current locked Swift
bridge names APIs introduced in the macOS 26 SDK at compile time. The build host
therefore needs:

- Xcode or Command Line Tools that provide macOS SDK 26 or later;
- `xcrun`, Swift, `lipo`, `codesign`, `otool`, `nm`, and `strings`; and
- Rust 1.88 or later.

Select the intended Xcode installation if more than one is installed, then
verify the host contract:

```bash
xcrun --sdk macosx --show-sdk-version
bash scripts/verify-macos-build-host.sh
```

The second command must report SDK 26 or later and deployment target 13.0.

Install the Rust toolchain and both macOS targets:

```bash
rustup toolchain install 1.88.0 --profile minimal --component rustfmt,clippy
rustup target add --toolchain 1.88.0 \
  aarch64-apple-darwin x86_64-apple-darwin
```

### 2. Build host-native development binaries

```bash
cargo +1.88.0 build --locked --release \
  --bin local-browser-bridge \
  --bin local-computer-helper
```

Outputs:

```text
target/release/local-browser-bridge
target/release/local-computer-helper
```

The raw helper is suitable for code-level development, but it is not the
packaged `Local Computer Helper.app` identity used for Screen Recording and
Accessibility. A permission result from the raw helper is not evidence for the
packaged TCC flow.

Run the server with:

```bash
./target/release/local-browser-bridge
```

For ordinary extension development, load the repository's `extension`
directory directly in Chrome or Edge.

### 3. Build the universal macOS package shape

The following commands reproduce the local package layout used by the release
workflow. They do not create GitHub provenance or release acceptance evidence:

```bash
set -euo pipefail
version="$(bash scripts/audit-versions.sh)"
stage="$(mktemp -d)"
trap 'rm -rf -- "$stage"' EXIT

cargo +1.88.0 build --locked --release --bin local-browser-bridge \
  --bin local-computer-helper --target aarch64-apple-darwin
cargo +1.88.0 build --locked --release --bin local-browser-bridge \
  --bin local-computer-helper --target x86_64-apple-darwin

mkdir -p "$stage/Local Computer Helper.app/Contents/MacOS" dist
cp LICENSE THIRD_PARTY_LICENSES.txt "$stage/"
chmod 644 "$stage/LICENSE" "$stage/THIRD_PARTY_LICENSES.txt"

lipo -create \
  target/aarch64-apple-darwin/release/local-browser-bridge \
  target/x86_64-apple-darwin/release/local-browser-bridge \
  -output "$stage/local-browser-bridge"
lipo -create \
  target/aarch64-apple-darwin/release/local-computer-helper \
  target/x86_64-apple-darwin/release/local-computer-helper \
  -output "$stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"

sed "s/@VERSION@/$version/g" packaging/macos/Info.plist.in \
  > "$stage/Local Computer Helper.app/Contents/Info.plist"
chmod 755 "$stage/local-browser-bridge" \
  "$stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"

codesign --force --sign - "$stage/local-browser-bridge"
codesign --verify --strict "$stage/local-browser-bridge"
codesign --force --deep --sign - "$stage/Local Computer Helper.app"
codesign --verify --deep --strict "$stage/Local Computer Helper.app"

bash scripts/verify-macos-artifacts.sh \
  "$version" \
  "$stage/local-browser-bridge" \
  "$stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"

COPYFILE_DISABLE=1 tar --format ustar --no-xattrs -czf \
  "dist/local-browser-bridge-v${version}-macos-universal.tar.gz" \
  -C "$stage" \
  local-browser-bridge "Local Computer Helper.app" \
  LICENSE THIRD_PARTY_LICENSES.txt
```

Output:

```text
dist/local-browser-bridge-vVERSION-macos-universal.tar.gz
```

Both executable slices have a macOS 13 deployment target. The app bundle is
ad-hoc signed, not Developer ID-signed or notarized, so local builds can require
fresh privacy grants.

### 4. Package the extension on macOS

macOS includes suitable `zip` and `unzip` commands:

```bash
bash scripts/package-extension.sh
```

Output:

```text
dist/local-browser-bridge-extension-vVERSION.zip
```

The script uses an exact allowlist, fixed timestamps, and byte-for-byte source
comparisons. It rejects missing or linked inputs and verifies the finished ZIP.

## Test levels

### Compile and library tests

These commands do not execute the Node-backed integration contracts:

```bash
cargo +1.88.0 build --locked --release \
  --bin local-browser-bridge --bin local-computer-helper
cargo +1.88.0 test --locked --lib
```

### Complete source checks

Install Node.js 24 first. On macOS and other Unix hosts, ensure `bash` and
`python3` are also available. Then run:

```bash
cargo +1.88.0 fmt --all -- --check
cargo +1.88.0 clippy --locked --all-targets -- -D warnings
cargo +1.88.0 test --locked --all-targets
bash scripts/audit-versions.sh
```

On Windows, use the explicit MSVC toolchain and target for the platform suite:

```powershell
cargo +1.88.0-x86_64-pc-windows-msvc fmt --all -- --check
cargo +1.88.0-x86_64-pc-windows-msvc clippy --locked `
  --target x86_64-pc-windows-msvc --all-targets -- -D warnings
cargo +1.88.0-x86_64-pc-windows-msvc test --locked `
  --target x86_64-pc-windows-msvc --all-targets
```

For the dependency-license gate:

```bash
cargo install cargo-about --locked --version 0.9.2 --features cli
bash scripts/check-licenses.sh
```

For documentation changes:

```bash
git diff --check
rg -nP '\p{Hangul}' README.md SECURITY.md docs extension public src tests
```

The second command must return no matches because repository and user-facing
text is English-only.

## Official release boundary

Do not rename a local build to look like a published release or describe it as
GitHub-attested. The official process additionally builds the frozen source on
GitHub-hosted Windows and macOS runners, packages the exact extension, generates
`SHA256SUMS.txt`, publishes build provenance, runs packaged acceptance gates,
creates an immutable GitHub Release, downloads every public asset again, and
re-verifies its contents and attestations.

See [Development](DEVELOPMENT.md) for protocol and live-evidence work. Install
only artifacts that actually appear on the public Releases page when release
provenance is required.
