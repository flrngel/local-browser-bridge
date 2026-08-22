#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

version="${1:-}"
assets_dir="${2:-dist}"
if [[ -z "$version" ]]; then
  echo "Usage: $0 VERSION [ASSETS_DIRECTORY]" >&2
  exit 1
fi
version="${version#v}"
bash scripts/audit-versions.sh "$version" >/dev/null

if [[ ! -d "$assets_dir" ]]; then
  echo "Release asset directory does not exist: $assets_dir" >&2
  exit 1
fi
assets_dir="$(cd "$assets_dir" && pwd -P)"

windows_server="$assets_dir/local-browser-bridge-v${version}-windows-x86_64.exe"
windows_helper="$assets_dir/local-computer-helper-v${version}-windows-x86_64.exe"
macos_archive="$assets_dir/local-browser-bridge-v${version}-macos-universal.tar.gz"
extension_archive="$assets_dir/local-browser-bridge-extension-v${version}.zip"
checksum_manifest="$assets_dir/SHA256SUMS.txt"

assets=("$windows_server" "$windows_helper" "$macos_archive" "$extension_archive")
for asset in "${assets[@]}"; do
  if [[ ! -f "$asset" || -L "$asset" || ! -s "$asset" ]]; then
    echo "Release asset is missing, empty, linked, or not a regular file: $asset" >&2
    exit 1
  fi
done

expected_release_listing="$(printf '%s\n' \
  "$(basename "$windows_server")" \
  "$(basename "$windows_helper")" \
  "$(basename "$macos_archive")" \
  "$(basename "$extension_archive")" \
  "$(basename "$checksum_manifest")" | LC_ALL=C sort)"
actual_release_listing="$(find "$assets_dir" -mindepth 1 -maxdepth 1 -exec basename {} \; | LC_ALL=C sort)"
if [[ "$actual_release_listing" != "$expected_release_listing" ]]; then
  echo "Release directory contains an unexpected file set." >&2
  diff -u <(printf '%s\n' "$expected_release_listing") <(printf '%s\n' "$actual_release_listing") || true
  exit 1
fi

for executable in "$windows_server" "$windows_helper"; do
  if [[ "$(od -An -tx1 -N2 "$executable" | tr -d ' \n')" != "4d5a" ]]; then
    echo "Windows executable is missing its MZ header: $executable" >&2
    exit 1
  fi
done

expected_extension_files=(background.js content.js dom-core.js frame-agent.js lib.js manifest.json popup.css popup.html popup.js LICENSE)
expected_extension_listing="$(printf '%s\n' "${expected_extension_files[@]}" | LC_ALL=C sort)"
actual_extension_listing="$(unzip -Z1 "$extension_archive" | LC_ALL=C sort)"
if [[ "$actual_extension_listing" != "$expected_extension_listing" ]]; then
  echo "Extension archive contains an unexpected file set." >&2
  diff -u <(printf '%s\n' "$expected_extension_listing") <(printf '%s\n' "$actual_extension_listing") || true
  exit 1
fi
extension_entry_types="$(zipinfo -l "$extension_archive" | awk '$1 ~ /^[bcdlps-]/ { print substr($1, 1, 1) }')"
if [[ "$(printf '%s\n' "$extension_entry_types" | wc -l | tr -d ' ')" != "${#expected_extension_files[@]}" ]] \
  || printf '%s\n' "$extension_entry_types" | grep -qv '^-$'; then
  echo "Extension archive contains a link or unsupported entry type." >&2
  exit 1
fi
unzip -tq "$extension_archive" >/dev/null
if ! unzip -p "$extension_archive" LICENSE | cmp -s - LICENSE; then
  echo "Extension archive project license differs from LICENSE." >&2
  exit 1
fi
archive_manifest_version="$(unzip -p "$extension_archive" manifest.json | sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
archive_library_version="$(unzip -p "$extension_archive" lib.js | sed -n 's/^export const VERSION = "\([^"]*\)";$/\1/p' | head -n 1)"
if [[ "$archive_manifest_version" != "$version" || "$archive_library_version" != "$version" ]]; then
  echo "Extension archive version does not match $version." >&2
  exit 1
fi

macos_listing="$(tar -tzf "$macos_archive")"
if printf '%s\n' "$macos_listing" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
  echo "macOS archive contains an unsafe path." >&2
  exit 1
fi
for required in \
  local-browser-bridge \
  LICENSE \
  THIRD_PARTY_LICENSES.txt \
  "Local Computer Helper.app/Contents/Info.plist" \
  "Local Computer Helper.app/Contents/MacOS/local-computer-helper"; do
  if ! printf '%s\n' "$macos_listing" | grep -Fxq "$required"; then
    echo "macOS archive is missing: $required" >&2
    exit 1
  fi
done
if [[ -n "$(printf '%s\n' "$macos_listing" | LC_ALL=C sort | uniq -d)" ]]; then
  echo "macOS archive contains duplicate paths." >&2
  exit 1
fi
if ! tar -tvzf "$macos_archive" | awk '
  substr($1, 1, 1) != "-" && substr($1, 1, 1) != "d" { exit 1 }
'; then
  echo "macOS archive contains a link or unsupported entry type." >&2
  exit 1
fi
expected_macos_listing="$(printf '%s\n' \
  local-browser-bridge \
  LICENSE \
  THIRD_PARTY_LICENSES.txt \
  "Local Computer Helper.app" \
  "Local Computer Helper.app/Contents" \
  "Local Computer Helper.app/Contents/Info.plist" \
  "Local Computer Helper.app/Contents/MacOS" \
  "Local Computer Helper.app/Contents/MacOS/local-computer-helper" \
  "Local Computer Helper.app/Contents/_CodeSignature" \
  "Local Computer Helper.app/Contents/_CodeSignature/CodeResources" | LC_ALL=C sort)"
actual_macos_listing="$(printf '%s\n' "$macos_listing" | sed 's:/$::' | LC_ALL=C sort)"
if [[ "$actual_macos_listing" != "$expected_macos_listing" ]]; then
  echo "macOS archive contains an unexpected file set." >&2
  diff -u <(printf '%s\n' "$expected_macos_listing") <(printf '%s\n' "$actual_macos_listing") || true
  exit 1
fi

mac_stage="$(mktemp -d)"
trap 'rm -rf "$mac_stage"' EXIT
tar -xzf "$macos_archive" -C "$mac_stage"
mac_server="$mac_stage/local-browser-bridge"
mac_helper="$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"
for executable in "$mac_server" "$mac_helper"; do
  if [[ ! -f "$executable" || -L "$executable" || ! -x "$executable" ]]; then
    echo "macOS binary is not executable: $executable" >&2
    exit 1
  fi
  magic="$(od -An -tx1 -N4 "$executable" | tr -d ' \n')"
  if [[ "$magic" != "cafebabe" && "$magic" != "cafebabf" ]]; then
    echo "macOS binary is not a universal Mach-O: $executable ($magic)" >&2
    exit 1
  fi
done
plist="$mac_stage/Local Computer Helper.app/Contents/Info.plist"
if [[ ! -f "$plist" || -L "$plist" ]]; then
  echo "macOS helper metadata is not a regular file." >&2
  exit 1
fi
for notice in LICENSE THIRD_PARTY_LICENSES.txt; do
  if [[ ! -f "$mac_stage/$notice" || -L "$mac_stage/$notice" ]] \
    || ! cmp -s "$mac_stage/$notice" "$notice"; then
    echo "macOS archive has a missing, linked, or changed notice: $notice" >&2
    exit 1
  fi
done
if [[ "$(grep -Fc "<string>$version</string>" "$plist")" -lt 2 ]]; then
  echo "macOS helper metadata does not contain version $version." >&2
  exit 1
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
  bash scripts/verify-macos-artifacts.sh "$version" "$mac_server" "$mac_helper"
fi

if [[ ! -f "$checksum_manifest" || -L "$checksum_manifest" || ! -s "$checksum_manifest" ]]; then
  echo "Release checksum manifest is missing, empty, linked, or not a regular file: $checksum_manifest" >&2
  exit 1
fi
expected_checksum_files="$(printf '%s\n' \
  "$(basename "$windows_server")" \
  "$(basename "$windows_helper")" \
  "$(basename "$macos_archive")" \
  "$(basename "$extension_archive")" | LC_ALL=C sort)"
actual_checksum_files="$(sed -n 's/^[[:xdigit:]]\{64\}[[:space:]][ *]\(.*\)$/\1/p' "$checksum_manifest" | LC_ALL=C sort)"
if [[ "$actual_checksum_files" != "$expected_checksum_files" ]]; then
  echo "Checksum manifest contains an unexpected file set." >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$assets_dir" && sha256sum --check SHA256SUMS.txt)
else
  (cd "$assets_dir" && shasum -a 256 -c SHA256SUMS.txt)
fi

echo "Verified release assets for $version."
