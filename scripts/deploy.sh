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

if [[ "$(uname -s)" == "Darwin" ]] && command -v brew >/dev/null 2>&1; then
  for formula in llvm lld; do
    formula_prefix="$(brew --prefix "$formula" 2>/dev/null || true)"
    if [[ -d "$formula_prefix/bin" ]]; then
      PATH="$formula_prefix/bin:$PATH"
    fi
  done
  export PATH
fi

windows_server_exe=""
windows_helper_exe=""
if [[ "${OS:-}" == "Windows_NT" ]]; then
  cargo build --locked --release --bins --target x86_64-pc-windows-msvc
  windows_server_exe="target/x86_64-pc-windows-msvc/release/local-browser-bridge.exe"
  windows_helper_exe="target/x86_64-pc-windows-msvc/release/local-computer-helper.exe"
elif command -v cargo-xwin >/dev/null 2>&1 || cargo xwin --version >/dev/null 2>&1; then
  cargo xwin build --locked --release --bins --target x86_64-pc-windows-msvc
  windows_server_exe="target/x86_64-pc-windows-msvc/release/local-browser-bridge.exe"
  windows_helper_exe="target/x86_64-pc-windows-msvc/release/local-computer-helper.exe"
elif command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  rustup target add x86_64-pc-windows-gnu
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    cargo build --locked --release --bins --target x86_64-pc-windows-gnu
  windows_server_exe="target/x86_64-pc-windows-gnu/release/local-browser-bridge.exe"
  windows_helper_exe="target/x86_64-pc-windows-gnu/release/local-computer-helper.exe"
else
  echo "Windows build unavailable. Use the tagged GitHub release workflow or install cargo-xwin." >&2
  exit 1
fi

windows_output="dist/local-browser-bridge-v${version}-windows-x86_64.exe"
windows_helper_output="dist/local-computer-helper-v${version}-windows-x86_64.exe"
cp "$windows_server_exe" "$windows_output"
cp "$windows_helper_exe" "$windows_helper_output"
for output in "$windows_output" "$windows_helper_output"; do
  if [[ "$(od -An -tx1 -N2 "$output" | tr -d ' \n')" != "4d5a" ]]; then
    echo "Windows executable verification failed for $output: missing MZ header." >&2
    exit 1
  fi
done

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "A macOS host is required for the universal binary. Use the tagged GitHub release workflow." >&2
  exit 1
fi
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --locked --release --bins --target aarch64-apple-darwin
cargo build --locked --release --bins --target x86_64-apple-darwin
mac_stage="$(mktemp -d)"
trap 'rm -rf "$mac_stage"' EXIT
mkdir -p "$mac_stage/Local Computer Helper.app/Contents/MacOS"
lipo -create \
  target/aarch64-apple-darwin/release/local-browser-bridge \
  target/x86_64-apple-darwin/release/local-browser-bridge \
  -output "$mac_stage/local-browser-bridge"
lipo -create \
  target/aarch64-apple-darwin/release/local-computer-helper \
  target/x86_64-apple-darwin/release/local-computer-helper \
  -output "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist.in > "$mac_stage/Local Computer Helper.app/Contents/Info.plist"
chmod 755 "$mac_stage/local-browser-bridge" "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"
for executable in "$mac_stage/local-browser-bridge" "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"; do
  architectures="$(lipo -archs "$executable")"
  if [[ "$architectures" != *"arm64"* || "$architectures" != *"x86_64"* ]]; then
    echo "macOS universal binary verification failed for $executable: $architectures" >&2
    exit 1
  fi
done
test "$("$mac_stage/local-browser-bridge" --version)" = "local-browser-bridge $version"
test "$("$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper" --version)" = "local-computer-helper $version"
codesign --force --deep --sign - "$mac_stage/Local Computer Helper.app"
codesign --verify --deep --strict "$mac_stage/Local Computer Helper.app"
mac_output="dist/local-browser-bridge-v${version}-macos-universal.tar.gz"
tar -czf "$mac_output" -C "$mac_stage" local-browser-bridge "Local Computer Helper.app"

checksum_output="dist/SHA256SUMS.txt"
assets=("$(basename "$windows_output")" "$(basename "$windows_helper_output")" "$(basename "$mac_output")" "$(basename "$extension_output")")
if command -v sha256sum >/dev/null 2>&1; then
  (cd dist && sha256sum "${assets[@]}") > "$checksum_output"
  (cd dist && sha256sum --check SHA256SUMS.txt)
else
  (cd dist && shasum -a 256 "${assets[@]}") > "$checksum_output"
  (cd dist && shasum -a 256 -c SHA256SUMS.txt)
fi
unzip -tq "$extension_output" >/dev/null
tar -tzf "$mac_output" | grep -Fxq local-browser-bridge
tar -tzf "$mac_output" | grep -Fxq "Local Computer Helper.app/Contents/MacOS/local-computer-helper"

echo "Created and verified Local Browser Bridge $version:"
printf '  %s\n' \
  "$project_root/$windows_output" \
  "$project_root/$windows_helper_output" \
  "$project_root/$mac_output" \
  "$extension_output" \
  "$project_root/$checksum_output"
