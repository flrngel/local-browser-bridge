#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)"
manifest_version="$(sed -n 's/.*"version": "\([^"]*\)".*/\1/p' extension/manifest.json | head -n 1)"
if [[ -z "$version" || "$version" != "$manifest_version" ]]; then
  echo "Server and extension versions are missing or do not match." >&2
  exit 1
fi

cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets

mkdir -p dist
extension_output="$project_root/dist/local-browser-bridge-extension-v${version}.zip"
bash scripts/package-extension.sh "$extension_output" >/dev/null

windows_exe=""
if [[ "${OS:-}" == "Windows_NT" ]]; then
  cargo build --locked --release --target x86_64-pc-windows-msvc
  windows_exe="target/x86_64-pc-windows-msvc/release/local-browser-bridge.exe"
elif command -v cargo-xwin >/dev/null 2>&1 || cargo xwin --version >/dev/null 2>&1; then
  cargo xwin build --locked --release --target x86_64-pc-windows-msvc
  windows_exe="target/x86_64-pc-windows-msvc/release/local-browser-bridge.exe"
elif command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  rustup target add x86_64-pc-windows-gnu
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    cargo build --locked --release --target x86_64-pc-windows-gnu
  windows_exe="target/x86_64-pc-windows-gnu/release/local-browser-bridge.exe"
else
  echo "Windows build unavailable. Use the tagged GitHub release workflow or install cargo-xwin." >&2
  exit 1
fi

windows_output="dist/local-browser-bridge-v${version}-windows-x86_64.exe"
cp "$windows_exe" "$windows_output"
if [[ "$(od -An -tx1 -N2 "$windows_output" | tr -d ' \n')" != "4d5a" ]]; then
  echo "Windows executable verification failed: missing MZ header." >&2
  exit 1
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "A macOS host is required for the universal binary. Use the tagged GitHub release workflow." >&2
  exit 1
fi
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --locked --release --target aarch64-apple-darwin
cargo build --locked --release --target x86_64-apple-darwin
mac_stage="$(mktemp -d)"
trap 'rm -rf "$mac_stage"' EXIT
lipo -create \
  target/aarch64-apple-darwin/release/local-browser-bridge \
  target/x86_64-apple-darwin/release/local-browser-bridge \
  -output "$mac_stage/local-browser-bridge"
chmod 755 "$mac_stage/local-browser-bridge"
architectures="$(lipo -archs "$mac_stage/local-browser-bridge")"
if [[ "$architectures" != *"arm64"* || "$architectures" != *"x86_64"* ]]; then
  echo "macOS universal binary verification failed: $architectures" >&2
  exit 1
fi
mac_output="dist/local-browser-bridge-v${version}-macos-universal.tar.gz"
tar -czf "$mac_output" -C "$mac_stage" local-browser-bridge

checksum_output="dist/SHA256SUMS.txt"
assets=("$(basename "$windows_output")" "$(basename "$mac_output")" "$(basename "$extension_output")")
if command -v sha256sum >/dev/null 2>&1; then
  (cd dist && sha256sum "${assets[@]}") > "$checksum_output"
  (cd dist && sha256sum --check SHA256SUMS.txt)
else
  (cd dist && shasum -a 256 "${assets[@]}") > "$checksum_output"
  (cd dist && shasum -a 256 -c SHA256SUMS.txt)
fi
unzip -tq "$extension_output" >/dev/null
tar -tzf "$mac_output" | grep -Fxq local-browser-bridge

echo "Created and verified Local Browser Bridge $version:"
printf '  %s\n' \
  "$project_root/$windows_output" \
  "$project_root/$mac_output" \
  "$extension_output" \
  "$project_root/$checksum_output"
