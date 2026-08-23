#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

version="$(bash scripts/audit-versions.sh)"
release_stage="$(mktemp -d)"
validation_stage="$(mktemp -d)"
trap 'rm -rf "$release_stage" "$validation_stage"' EXIT

node --check scripts/wait-macos-pointer-concurrency-handoff.mjs
node scripts/wait-macos-pointer-concurrency-handoff.mjs --mode self-test
node --check scripts/finalize-macos-acceptance.mjs
node scripts/finalize-macos-acceptance.mjs --self-test
node --check evidence/v0.12.12/computer/helper-evidence-rig.mjs
node evidence/v0.12.12/computer/helper-evidence-rig.mjs --self-test
bash -n scripts/fetch-verify-release-candidate.sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
bash scripts/check-licenses.sh

extension_output="$release_stage/local-browser-bridge-extension-v${version}.zip"
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
else
  echo "Official Windows artifacts require the x86_64-pc-windows-msvc target. Use the tagged GitHub release workflow or install cargo-xwin." >&2
  exit 1
fi

windows_output="$release_stage/local-browser-bridge-v${version}-windows-x86_64.exe"
windows_helper_output="$release_stage/local-computer-helper-v${version}-windows-x86_64.exe"
cp "$windows_server_exe" "$windows_output"
cp "$windows_helper_exe" "$windows_helper_output"
for output in "$windows_output" "$windows_helper_output"; do
  if [[ "$(od -An -tx1 -N2 "$output" | tr -d ' \n')" != "4d5a" ]]; then
    echo "Windows executable verification failed for $output: missing MZ header." >&2
    exit 1
  fi
done
if [[ "${OS:-}" == "Windows_NT" ]]; then
  pwsh -NoProfile -File scripts/verify-windows-artifacts.ps1 \
    -Version "$version" \
    -ServerPath "$windows_output" \
    -HelperPath "$windows_helper_output"
else
  if ! command -v llvm-readobj >/dev/null 2>&1; then
    echo "llvm-readobj is required to inspect cross-compiled Windows release artifacts." >&2
    exit 1
  fi
  for output in "$windows_output" "$windows_helper_output"; do
    pe_report="$(llvm-readobj --coff-imports --coff-resources "$output")"
    if grep -Eqi 'VCRUNTIME|MSVCP|api-ms-win-crt' <<<"$pe_report"; then
      echo "Windows executable still imports a separately distributed Visual C++ runtime: $output" >&2
      exit 1
    fi
    if ! grep -Eqi 'MANIFEST|RT_MANIFEST|Type: 0x18|Type: 24' <<<"$pe_report"; then
      echo "Windows executable is missing its embedded application manifest: $output" >&2
      exit 1
    fi
  done
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "A macOS host is required for the universal binary. Use the tagged GitHub release workflow." >&2
  exit 1
fi
bash scripts/verify-macos-build-host.sh
xcrun swiftc -typecheck evidence/v0.12.12/computer/HelperEvidenceFixture.swift
xcrun swiftc -typecheck evidence/v0.12.12/computer/SystemProbe.swift
xcrun swiftc -typecheck evidence/v0.12.12/computer/PointerHandoff.swift
pointer_handoff_self_test="$validation_stage/lbb-pointer-handoff-self-test"
xcrun swiftc evidence/v0.12.12/computer/PointerHandoff.swift -o "$pointer_handoff_self_test"
"$pointer_handoff_self_test" --self-test
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --locked --release --bins --target aarch64-apple-darwin
cargo build --locked --release --bins --target x86_64-apple-darwin
mac_stage="$(mktemp -d)"
trap 'rm -rf "$release_stage" "$validation_stage" "$mac_stage"' EXIT
mkdir -p "$mac_stage/Local Computer Helper.app/Contents/MacOS"
cp LICENSE THIRD_PARTY_LICENSES.txt "$mac_stage/"
chmod 644 "$mac_stage/LICENSE" "$mac_stage/THIRD_PARTY_LICENSES.txt"
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
for executable in "$mac_stage/local-browser-bridge" "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"; do
  license_report="$("$executable" --licenses)"
  grep -Fq 'Local Browser Bridge third-party licenses' <<<"$license_report"
  grep -Fq 'MIT License' <<<"$license_report"
  grep -Fq 'Apache License' <<<"$license_report"
done
codesign --force --sign - "$mac_stage/local-browser-bridge"
codesign --verify --strict "$mac_stage/local-browser-bridge"
codesign --force --deep --sign - "$mac_stage/Local Computer Helper.app"
codesign --verify --deep --strict "$mac_stage/Local Computer Helper.app"
bash scripts/verify-macos-artifacts.sh \
  "$version" \
  "$mac_stage/local-browser-bridge" \
  "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"
mac_output="$release_stage/local-browser-bridge-v${version}-macos-universal.tar.gz"
COPYFILE_DISABLE=1 tar --format ustar --no-xattrs -czf "$mac_output" -C "$mac_stage" local-browser-bridge "Local Computer Helper.app" LICENSE THIRD_PARTY_LICENSES.txt

checksum_output="$release_stage/SHA256SUMS.txt"
assets=("$(basename "$windows_output")" "$(basename "$windows_helper_output")" "$(basename "$mac_output")" "$(basename "$extension_output")")
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$release_stage" && sha256sum "${assets[@]}") > "$checksum_output"
  (cd "$release_stage" && sha256sum --check SHA256SUMS.txt)
else
  (cd "$release_stage" && shasum -a 256 "${assets[@]}") > "$checksum_output"
  (cd "$release_stage" && shasum -a 256 -c SHA256SUMS.txt)
fi
unzip -tq "$extension_output" >/dev/null
tar -tzf "$mac_output" | grep -Fxq local-browser-bridge
tar -tzf "$mac_output" | grep -Fxq "Local Computer Helper.app/Contents/MacOS/local-computer-helper"
bash scripts/verify-release-assets.sh "$version" "$release_stage"

mkdir -p dist
for asset in "${assets[@]}" SHA256SUMS.txt; do
  cp "$release_stage/$asset" "dist/$asset"
done

echo "Created and verified Local Browser Bridge $version:"
printf '  %s\n' \
  "$project_root/dist/$(basename "$windows_output")" \
  "$project_root/dist/$(basename "$windows_helper_output")" \
  "$project_root/dist/$(basename "$mac_output")" \
  "$project_root/dist/$(basename "$extension_output")" \
  "$project_root/dist/$(basename "$checksum_output")"
