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

mkdir -p dist
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
  echo "A Windows cross-compiler is required. Install cargo-xwin or mingw-w64, then run deploy again." >&2
  exit 1
fi

exe_output="dist/local-browser-bridge-v${version}-windows-x86_64.exe"
extension_output="$project_root/dist/local-browser-bridge-extension-v${version}.zip"
cp "$windows_exe" "$exe_output"
rm -f "$extension_output"
(
  cd extension
  zip -q -r "$extension_output" .
)

if [[ "$(od -An -tx1 -N2 "$exe_output" | tr -d ' \n')" != "4d5a" ]]; then
  echo "Windows executable verification failed: missing MZ header." >&2
  exit 1
fi
unzip -tq "$extension_output" >/dev/null
unzip -l "$extension_output" | grep -q 'manifest.json'

checksum_output="dist/SHA256SUMS.txt"
if command -v sha256sum >/dev/null 2>&1; then
  (cd dist && sha256sum "$(basename "$exe_output")" "$(basename "$extension_output")") > "$checksum_output"
else
  (cd dist && shasum -a 256 "$(basename "$exe_output")" "$(basename "$extension_output")") > "$checksum_output"
fi

echo "Created and verified:"
echo "  $project_root/$exe_output"
echo "  $extension_output"
echo "  $project_root/$checksum_output"
