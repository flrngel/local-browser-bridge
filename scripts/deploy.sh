#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

release_blocker="$project_root/RELEASE_BLOCKED"
if [[ -e "$release_blocker" || -L "$release_blocker" ]]; then
  echo "Release is blocked by RELEASE_BLOCKED; resolve its recorded source gate and remove it in a reviewed unblock commit." >&2
  exit 1
fi

version="$(bash scripts/audit-versions.sh)"
dist_dir="$project_root/dist"
release_stage=""
validation_stage=""
mac_stage=""
publish_stage=""
dist_rollback_parent=""
dist_rollback_path=""
dist_replacement_pending=0
dist_publish_installing=0
dist_publish_installed=0
dist_publish_verified=0
failed_publish_parent=""
failed_publish_path=""

is_recognized_generated_release_asset() {
  local name="$1"
  if [[ "$name" == "SHA256SUMS.txt" ]]; then
    return 0
  fi
  if [[ "$name" =~ ^local-browser-bridge-v[0-9]+\.[0-9]+\.[0-9]+-windows-x86_64\.exe$ ]] \
    || [[ "$name" =~ ^local-computer-helper-v[0-9]+\.[0-9]+\.[0-9]+-windows-x86_64\.exe$ ]] \
    || [[ "$name" =~ ^local-browser-bridge-v[0-9]+\.[0-9]+\.[0-9]+-macos-universal\.tar\.gz$ ]] \
    || [[ "$name" =~ ^local-browser-bridge-extension-v[0-9]+\.[0-9]+\.[0-9]+\.zip$ ]]; then
    return 0
  fi
  return 1
}

validate_replaceable_dist() {
  local candidate="$1"
  if [[ -L "$candidate" ]]; then
    echo "Refusing to replace a linked dist path: $candidate" >&2
    return 1
  fi
  if [[ ! -e "$candidate" ]]; then
    return 0
  fi
  if [[ ! -d "$candidate" || ! -r "$candidate" || ! -x "$candidate" ]]; then
    echo "Refusing to replace dist because it is not a readable, searchable directory: $candidate" >&2
    return 1
  fi

  local nullglob_was_set=0
  local dotglob_was_set=0
  shopt -q nullglob && nullglob_was_set=1
  shopt -q dotglob && dotglob_was_set=1
  shopt -s nullglob dotglob
  local entries=("$candidate"/*)
  (( nullglob_was_set == 1 )) || shopt -u nullglob
  (( dotglob_was_set == 1 )) || shopt -u dotglob

  local entry name
  for entry in "${entries[@]}"; do
    name="${entry##*/}"
    if [[ -L "$entry" || ! -f "$entry" ]]; then
      echo "Refusing to replace dist because it contains a linked or non-file entry: $name" >&2
      return 1
    fi
    if ! is_recognized_generated_release_asset "$name"; then
      echo "Refusing to replace dist because it contains an unrecognized entry: $name" >&2
      return 1
    fi
  done
}

restore_dist_rollback() {
  if (( dist_replacement_pending == 0 )); then
    return 0
  fi
  if [[ -e "$dist_dir" || -L "$dist_dir" ]]; then
    echo "Could not restore the previous dist because the destination is occupied; rollback retained at $dist_rollback_path" >&2
    return 1
  fi
  if ! mv "$dist_rollback_path" "$dist_dir"; then
    echo "Could not restore the previous dist; rollback retained at $dist_rollback_path" >&2
    return 1
  fi
  dist_replacement_pending=0
}

quarantine_unverified_dist() {
  if (( dist_publish_installed == 0 && dist_publish_installing == 0 )); then
    return 0
  fi
  if (( dist_publish_installing == 1 )) \
    && [[ -e "$publish_stage" || -L "$publish_stage" ]]; then
    return 0
  fi
  if [[ ! -e "$dist_dir" && ! -L "$dist_dir" ]]; then
    dist_publish_installing=0
    dist_publish_installed=0
    return 0
  fi
  if ! failed_publish_parent="$(mktemp -d "$project_root/.dist-failed.XXXXXX")"; then
    echo "Could not allocate a quarantine for the unverified dist replacement." >&2
    return 1
  fi
  failed_publish_path="$failed_publish_parent/dist"
  if ! mv "$dist_dir" "$failed_publish_path"; then
    echo "Could not quarantine the unverified dist replacement; the previous rollback remains at $dist_rollback_path" >&2
    return 1
  fi
  dist_publish_installing=0
  dist_publish_installed=0
  echo "Preserved the unverified dist replacement at $failed_publish_path" >&2
}

cleanup() {
  local status="$?"
  trap - EXIT
  set +e
  if (( dist_publish_verified == 0 )); then
    quarantine_unverified_dist || status=1
  else
    dist_replacement_pending=0
  fi
  if (( dist_replacement_pending == 1 )); then
    restore_dist_rollback || status=1
  fi
  local temporary_path
  for temporary_path in "$release_stage" "$validation_stage" "$mac_stage" "$publish_stage"; do
    if [[ -n "$temporary_path" && ( -e "$temporary_path" || -L "$temporary_path" ) ]]; then
      rm -rf "$temporary_path"
    fi
  done
  if [[ -n "$dist_rollback_parent" && -d "$dist_rollback_parent" ]]; then
    if [[ -n "$dist_rollback_path" && ( -e "$dist_rollback_path" || -L "$dist_rollback_path" ) ]]; then
      echo "Preserved the previous dist rollback at $dist_rollback_path" >&2
    else
      rmdir "$dist_rollback_parent" 2>/dev/null || true
    fi
  fi
  if [[ -n "$failed_publish_path" && ( -e "$failed_publish_path" || -L "$failed_publish_path" ) ]]; then
    echo "Retained the failed dist publication at $failed_publish_path" >&2
  fi
  exit "$status"
}

trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

release_stage="$(mktemp -d)"
validation_stage="$(mktemp -d)"
validate_replaceable_dist "$dist_dir"

node --check scripts/wait-macos-app-share-concurrency-handoff.mjs
node scripts/wait-macos-app-share-concurrency-handoff.mjs --mode self-test
node --check scripts/finalize-macos-acceptance.mjs
node scripts/finalize-macos-acceptance.mjs --self-test
bash -n scripts/finalize-macos-acceptance.sh
bash scripts/finalize-macos-acceptance.sh --self-test
node --check evidence/v0.12.59/computer/helper-evidence-rig.mjs
node evidence/v0.12.59/computer/helper-evidence-rig.mjs --self-test
bash -n scripts/fetch-verify-release-candidate.sh
bash scripts/fetch-verify-release-candidate.sh --self-test
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

windows_desktop_exe=""
windows_helper_exe=""
if [[ "${OS:-}" == "Windows_NT" ]]; then
  cargo build --locked --release --bins --target x86_64-pc-windows-msvc
  windows_desktop_exe="target/x86_64-pc-windows-msvc/release/local-browser-bridge-desktop.exe"
  windows_helper_exe="target/x86_64-pc-windows-msvc/release/local-computer-helper.exe"
elif command -v cargo-xwin >/dev/null 2>&1 || cargo xwin --version >/dev/null 2>&1; then
  cargo xwin build --locked --release --bins --target x86_64-pc-windows-msvc
  windows_desktop_exe="target/x86_64-pc-windows-msvc/release/local-browser-bridge-desktop.exe"
  windows_helper_exe="target/x86_64-pc-windows-msvc/release/local-computer-helper.exe"
else
  echo "Official Windows artifacts require the x86_64-pc-windows-msvc target. Use the tagged GitHub release workflow or install cargo-xwin." >&2
  exit 1
fi

windows_output="$release_stage/local-browser-bridge-v${version}-windows-x86_64.exe"
windows_helper_output="$release_stage/local-computer-helper-v${version}-windows-x86_64.exe"
cp "$windows_desktop_exe" "$windows_output"
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
    pe_report="$(llvm-readobj --file-headers --coff-imports --coff-resources "$output")"
    if grep -Eqi 'VCRUNTIME|MSVCP|api-ms-win-crt' <<<"$pe_report"; then
      echo "Windows executable still imports a separately distributed Visual C++ runtime: $output" >&2
      exit 1
    fi
    if ! grep -Eqi 'MANIFEST|RT_MANIFEST|Type: 0x18|Type: 24' <<<"$pe_report"; then
      echo "Windows executable is missing its embedded application manifest: $output" >&2
      exit 1
    fi
  done
  llvm-readobj --file-headers "$windows_output" | grep -Fq 'IMAGE_SUBSYSTEM_WINDOWS_GUI' \
    || { echo "Windows Desktop Host is not linked as a GUI-subsystem executable." >&2; exit 1; }
fi

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "A macOS host is required for the universal binary. Use the tagged GitHub release workflow." >&2
  exit 1
fi
bash scripts/verify-macos-build-host.sh
xcrun swiftc -typecheck evidence/v0.12.59/computer/HelperEvidenceFixture.swift
xcrun swiftc -typecheck evidence/v0.12.59/computer/SystemProbe.swift
bash scripts/verify-macos-app-share-handoff-self-test.sh "$version"
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --locked --release --bins --target aarch64-apple-darwin
cargo build --locked --release --bins --target x86_64-apple-darwin
mac_stage="$(mktemp -d)"
mkdir -p \
  "$mac_stage/Local Browser Bridge.app/Contents/MacOS" \
  "$mac_stage/Local Computer Helper.app/Contents/MacOS"
cp LICENSE THIRD_PARTY_LICENSES.txt "$mac_stage/"
chmod 644 "$mac_stage/LICENSE" "$mac_stage/THIRD_PARTY_LICENSES.txt"
lipo -create \
  target/aarch64-apple-darwin/release/local-browser-bridge \
  target/x86_64-apple-darwin/release/local-browser-bridge \
  -output "$mac_stage/local-browser-bridge"
lipo -create \
  target/aarch64-apple-darwin/release/local-browser-bridge-desktop \
  target/x86_64-apple-darwin/release/local-browser-bridge-desktop \
  -output "$mac_stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop"
lipo -create \
  target/aarch64-apple-darwin/release/local-computer-helper \
  target/x86_64-apple-darwin/release/local-computer-helper \
  -output "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist.in > "$mac_stage/Local Computer Helper.app/Contents/Info.plist"
sed "s/@VERSION@/$version/g" packaging/macos/DesktopInfo.plist.in > "$mac_stage/Local Browser Bridge.app/Contents/Info.plist"
chmod 755 \
  "$mac_stage/local-browser-bridge" \
  "$mac_stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop" \
  "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"
for executable in \
  "$mac_stage/local-browser-bridge" \
  "$mac_stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop" \
  "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"; do
  architectures="$(lipo -archs "$executable")"
  if [[ "$architectures" != *"arm64"* || "$architectures" != *"x86_64"* ]]; then
    echo "macOS universal binary verification failed for $executable: $architectures" >&2
    exit 1
  fi
done
test "$("$mac_stage/local-browser-bridge" --version)" = "local-browser-bridge $version"
test "$("$mac_stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop" --version)" = "local-browser-bridge-desktop $version"
test "$("$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper" --version)" = "local-computer-helper $version"
for executable in \
  "$mac_stage/local-browser-bridge" \
  "$mac_stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop" \
  "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"; do
  license_report="$("$executable" --licenses)"
  grep -Fq 'Local Browser Bridge third-party licenses' <<<"$license_report"
  grep -Fq 'MIT License' <<<"$license_report"
  grep -Fq 'Apache License' <<<"$license_report"
done
codesign --force --sign - "$mac_stage/local-browser-bridge"
codesign --verify --strict "$mac_stage/local-browser-bridge"
codesign --force --deep --sign - "$mac_stage/Local Browser Bridge.app"
codesign --verify --deep --strict "$mac_stage/Local Browser Bridge.app"
codesign --force --deep --sign - "$mac_stage/Local Computer Helper.app"
codesign --verify --deep --strict "$mac_stage/Local Computer Helper.app"
bash scripts/verify-macos-artifacts.sh \
  "$version" \
  "$mac_stage/local-browser-bridge" \
  "$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper" \
  "$mac_stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop"
mac_output="$release_stage/local-browser-bridge-v${version}-macos-universal.tar.gz"
COPYFILE_DISABLE=1 tar --format ustar --no-xattrs -czf "$mac_output" -C "$mac_stage" local-browser-bridge "Local Browser Bridge.app" "Local Computer Helper.app" LICENSE THIRD_PARTY_LICENSES.txt
tar -tzf "$mac_output" | sed 's:/$::' | LC_ALL=C sort > "$validation_stage/macos-archive.txt"
LC_ALL=C sort packaging/macos/release-archive-inventory.txt > "$validation_stage/expected-macos-archive.txt"
cmp -s "$validation_stage/macos-archive.txt" "$validation_stage/expected-macos-archive.txt"

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
tar -tzf "$mac_output" | grep -Fxq "Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop"
tar -tzf "$mac_output" | grep -Fxq "Local Computer Helper.app/Contents/MacOS/local-computer-helper"
bash scripts/verify-release-assets.sh "$version" "$release_stage"

publish_stage="$(mktemp -d "$project_root/.dist-publish.XXXXXX")"
for asset in "${assets[@]}" SHA256SUMS.txt; do
  cp "$release_stage/$asset" "$publish_stage/$asset"
done
bash scripts/verify-release-assets.sh "$version" "$publish_stage"

validate_replaceable_dist "$dist_dir"
if [[ -e "$dist_dir" ]]; then
  dist_rollback_parent="$(mktemp -d "$project_root/.dist-rollback.XXXXXX")"
  dist_rollback_path="$dist_rollback_parent/dist"
  if ! mv "$dist_dir" "$dist_rollback_path"; then
    echo "Failed to move the previous dist into its rollback directory." >&2
    exit 1
  fi
  dist_replacement_pending=1
  validate_replaceable_dist "$dist_rollback_path"
fi
if [[ -e "$dist_dir" || -L "$dist_dir" ]]; then
  echo "Refusing to publish because dist changed during replacement." >&2
  restore_dist_rollback || true
  exit 1
fi
dist_publish_installing=1
if ! mv "$publish_stage" "$dist_dir"; then
  dist_publish_installing=0
  echo "Failed to install the verified dist replacement." >&2
  restore_dist_rollback || true
  exit 1
fi
dist_publish_installed=1
dist_publish_installing=0
publish_stage=""
if ! bash scripts/verify-release-assets.sh "$version" "$dist_dir"; then
  echo "The installed dist replacement failed verification." >&2
  quarantine_unverified_dist || true
  restore_dist_rollback || true
  exit 1
fi
dist_publish_verified=1
dist_publish_installed=0
dist_replacement_pending=0
if [[ -n "$dist_rollback_parent" ]]; then
  rm -rf "$dist_rollback_parent"
  dist_rollback_parent=""
  dist_rollback_path=""
fi

echo "Created and verified Local Browser Bridge $version:"
printf '  %s\n' \
  "$project_root/dist/$(basename "$windows_output")" \
  "$project_root/dist/$(basename "$windows_helper_output")" \
  "$project_root/dist/$(basename "$mac_output")" \
  "$project_root/dist/$(basename "$extension_output")" \
  "$project_root/dist/$(basename "$checksum_output")"
