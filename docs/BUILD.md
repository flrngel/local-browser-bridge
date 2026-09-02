# Building from source

This guide builds the current source tree. A local build is a development
artifact: it is not a published GitHub Release, has no release attestation, and
has not passed the repository's packaged acceptance gates. End users who want a
stable, verified package should use the [Windows](INSTALL_WINDOWS.md) or
[macOS](INSTALL_MACOS.md) installation guide.

The product has no Node.js runtime dependency. Its primary implementation is
Rust; macOS builds also compile and link the repository's locked Swift capture
bridge. Node.js 24 is required only when running the complete developer
contract suite.

## Build matrix

| Target | Build host | Result |
|---|---|---|
| Windows x86_64 | 64-bit Windows with MSVC and Windows SDK | Tray Desktop Host, console server, and helper `.exe` files |
| macOS host architecture | macOS with SDK 26+ | Menu-bar Desktop Host, raw development server, and helper binaries |
| macOS universal | macOS with SDK 26+ | Universal menu-bar app, console server, and `Local Computer Helper.app` archive |
| Chromium extension | Any host for direct unpacked use; Bash plus `zip`/`unzip` for the deterministic ZIP | Manifest V3 unpacked directory or ZIP |

The project distributes Windows and macOS packages. A successful compile on an
unlisted host does not make its computer helper a supported platform.

## Common requirements

For a native compile:

- [Git](https://git-scm.com/downloads)
- [`rustup`, Cargo, and Rust](https://rust-lang.org/tools/install/) 1.88 or later
- the target platform's native SDK and linker

Install `rustup` from the official Rust page before running the commands below.
On macOS, the official installer command is:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

On Windows, use the x64 `rustup-init.exe` linked from the official Rust page,
then open a fresh PowerShell window. Do not install a separate system Cargo or
Rust package alongside `rustup`.

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

- [Git for Windows](https://git-scm.com/install/windows);
- Visual Studio 2022 or Visual Studio 2022 Build Tools;
- the **Desktop development with C++** workload, including MSVC v143 x64/x86
  build tools; and
- Windows 11 SDK 10.0.26100 or a newer supported stable Windows 11 SDK.

Microsoft's [Visual Studio 2022 Build Tools component
guide](https://learn.microsoft.com/en-us/visualstudio/install/workload-component-id-vs-build-tools?view=vs-2022)
lists the **Desktop development with C++** workload, MSVC v143 tools, and
Windows SDK components. The full Visual Studio IDE is optional; the standalone
Build Tools are sufficient. Install Rust with the official x64
`rustup-init.exe` after the C++ workload is present.

The SDK provides `mt.exe`; the Visual C++ tools provide `dumpbin.exe`. The
repository's artifact verifier uses both. The Windows target embeds an
`asInvoker`, `uiAccess=false`, Per-Monitor-V2 manifest and uses the static C
runtime, so a separate Visual C++ Redistributable is not part of the package.

Install Rust's MSVC target:

```powershell
rustup toolchain install 1.88.0-x86_64-pc-windows-msvc `
  --profile minimal --component rustfmt,clippy
```

Confirm the command-line tools from a new PowerShell window:

```powershell
git --version
rustup --version
cargo +1.88.0-x86_64-pc-windows-msvc --version
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

### 3. Build the three product executables

```powershell
cargo +1.88.0-x86_64-pc-windows-msvc build --locked --release `
  --target x86_64-pc-windows-msvc `
  --bin local-browser-bridge-desktop `
  --bin local-browser-bridge `
  --bin local-computer-helper
```

Outputs:

```text
target/x86_64-pc-windows-msvc/release/local-browser-bridge.exe
target/x86_64-pc-windows-msvc/release/local-browser-bridge-desktop.exe
target/x86_64-pc-windows-msvc/release/local-computer-helper.exe
```

The `mock-extension` binary is development tooling and is not a release asset.

### 4. Verify the development executables

```powershell
$version = (Select-String -Path Cargo.toml `
  -Pattern '^version = "([^"]+)"$').Matches[0].Groups[1].Value
$desktop = "target/x86_64-pc-windows-msvc/release/local-browser-bridge-desktop.exe"
$helper = "target/x86_64-pc-windows-msvc/release/local-computer-helper.exe"

& .\scripts\verify-windows-artifacts.ps1 `
  -Version $version `
  -ServerPath $desktop `
  -HelperPath $helper
```

The verifier checks the PE architecture, embedded manifest, VERSIONINFO,
static-runtime imports, helper input API surface, reported version, and embedded
licenses. It executes only the just-built local files with bounded informational
flags.

### 5. Run the development build

Run the normal tray application:

```powershell
& .\target\x86_64-pc-windows-msvc\release\local-browser-bridge-desktop.exe
```

Use **Start Computer Helper** from the tray menu only when desktop control is
needed. For headless server development, run the separate console binary:

```powershell
& .\target\x86_64-pc-windows-msvc\release\local-browser-bridge.exe
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

- [Xcode or Command Line Tools](https://developer.apple.com/download/) that
  provide macOS SDK 26 or later;
- `xcrun`, Swift, `lipo`, `codesign`, `otool`, `nm`, and `strings`; and
- Rust 1.88 or later.

Apple documents both the full Xcode install and
[`xcode-select --install`](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools).
The command-line prompt alone is not enough if it installs an older SDK: verify
the SDK version below before building.

Select the intended Xcode installation if more than one is installed, then
verify the SDK and deployment-target contract:

```bash
xcrun --sdk macosx --show-sdk-version
bash scripts/verify-macos-build-host.sh
```

The second command is a build-host preflight only: it checks Darwin, the active
SDK, Swift availability, and the configured deployment target; it neither
builds nor verifies an artifact. It must report SDK 26 or later and deployment
target 13.0. The later `verify-macos-artifacts.sh` command validates the built
server and helper slices. The build host must also run the Xcode or Command Line
Tools release that supplies that SDK; consult Apple's current [Xcode system
requirements](https://developer.apple.com/xcode/system-requirements/) for the
required host macOS version.

Confirm every packaging tool before a universal build:

```bash
for tool in xcrun swift lipo codesign otool nm strings zip unzip; do
  command -v "$tool" >/dev/null || { echo "Missing required tool: $tool" >&2; exit 1; }
done
```

Install the Rust toolchain and both macOS targets:

```bash
rustup toolchain install 1.88.0 --profile minimal --component rustfmt,clippy
rustup target add --toolchain 1.88.0 \
  aarch64-apple-darwin x86_64-apple-darwin
```

Confirm the basic tools:

```bash
git --version
rustup --version
cargo +1.88.0 --version
```

### 2. Clone and verify the source

```bash
git clone https://github.com/flrngel/local-browser-bridge.git
cd local-browser-bridge
git status --porcelain=v2 --untracked-files=all
git ls-files --deleted
```

The final two commands must produce no output.

### 3. Build host-native development binaries

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

### 4. Build the universal macOS package shape

The following commands reproduce the local package layout used by the release
workflow. They do not create GitHub provenance or release acceptance evidence:

```bash
set -euo pipefail
version="$(bash scripts/audit-versions.sh)"
stage="$(mktemp -d)"
trap 'rm -rf -- "$stage"' EXIT

cargo +1.88.0 build --locked --release --bin local-browser-bridge \
  --bin local-browser-bridge-desktop --bin local-computer-helper \
  --target aarch64-apple-darwin
cargo +1.88.0 build --locked --release --bin local-browser-bridge \
  --bin local-browser-bridge-desktop --bin local-computer-helper \
  --target x86_64-apple-darwin

mkdir -p "$stage/Local Browser Bridge.app/Contents/MacOS" \
  "$stage/Local Computer Helper.app/Contents/MacOS" dist
cp LICENSE THIRD_PARTY_LICENSES.txt "$stage/"
chmod 644 "$stage/LICENSE" "$stage/THIRD_PARTY_LICENSES.txt"

lipo -create \
  target/aarch64-apple-darwin/release/local-browser-bridge \
  target/x86_64-apple-darwin/release/local-browser-bridge \
  -output "$stage/local-browser-bridge"
lipo -create \
  target/aarch64-apple-darwin/release/local-browser-bridge-desktop \
  target/x86_64-apple-darwin/release/local-browser-bridge-desktop \
  -output "$stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop"
lipo -create \
  target/aarch64-apple-darwin/release/local-computer-helper \
  target/x86_64-apple-darwin/release/local-computer-helper \
  -output "$stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"

sed "s/@VERSION@/$version/g" packaging/macos/Info.plist.in \
  > "$stage/Local Computer Helper.app/Contents/Info.plist"
sed "s/@VERSION@/$version/g" packaging/macos/DesktopInfo.plist.in \
  > "$stage/Local Browser Bridge.app/Contents/Info.plist"
chmod 755 "$stage/local-browser-bridge" \
  "$stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop" \
  "$stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"

codesign --force --sign - "$stage/local-browser-bridge"
codesign --verify --strict "$stage/local-browser-bridge"
codesign --force --deep --sign - "$stage/Local Browser Bridge.app"
codesign --verify --deep --strict "$stage/Local Browser Bridge.app"
codesign --force --deep --sign - "$stage/Local Computer Helper.app"
codesign --verify --deep --strict "$stage/Local Computer Helper.app"

bash scripts/verify-macos-artifacts.sh \
  "$version" \
  "$stage/local-browser-bridge" \
  "$stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper" \
  "$stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop"

COPYFILE_DISABLE=1 tar --format ustar --no-xattrs -czf \
  "dist/local-browser-bridge-v${version}-macos-universal.tar.gz" \
  -C "$stage" \
  local-browser-bridge "Local Browser Bridge.app" "Local Computer Helper.app" \
  LICENSE THIRD_PARTY_LICENSES.txt
```

Output:

```text
dist/local-browser-bridge-vVERSION-macos-universal.tar.gz
```

All executable slices have a macOS 13 deployment target. Both app bundles are
ad-hoc signed, not Developer ID-signed or notarized, so local builds can require
fresh privacy grants.

### 5. Package the extension on macOS

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
  --bin local-browser-bridge-desktop --bin local-browser-bridge \
  --bin local-computer-helper
cargo +1.88.0 test --locked --lib
```

### Complete source checks

Install [Node.js 24](https://nodejs.org/en/download/archive/v24) first. Confirm
that `node --version` reports `v24.x`; npm packages are not required. On macOS
and other Unix hosts, ensure `bash` and `python3` are also available. Then run:

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

See [Development](maintainers/DEVELOPMENT.md) for protocol and live-evidence work. Install
only artifacts that actually appear on the public Releases page when release
provenance is required.
