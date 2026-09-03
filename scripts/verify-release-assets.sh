#!/usr/bin/env bash
set -euo pipefail
umask 077

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

version="${1:-}"
assets_dir="${2:-dist}"
verification_mode="runtime"
if (( $# > 3 )); then
  echo "Usage: $0 VERSION [ASSETS_DIRECTORY] [--static-only]" >&2
  exit 1
elif (( $# == 3 )) && [[ "$3" == "--static-only" ]]; then
  verification_mode="static-only"
elif (( $# == 3 )); then
  echo "Usage: $0 VERSION [ASSETS_DIRECTORY] [--static-only]" >&2
  exit 1
fi
if [[ -z "$version" ]]; then
  echo "Usage: $0 VERSION [ASSETS_DIRECTORY] [--static-only]" >&2
  exit 1
fi
version="${version#v}"
for command_name in python3 shasum; do
  command -v "$command_name" >/dev/null || {
    echo "Required release inspection command is unavailable: $command_name" >&2
    exit 1
  }
done
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

expected_extension_files=(background.js content.js dom-core.js frame-agent.js lib.js manifest.json pair.html pair.js popup.css popup.html popup.js LICENSE)
extension_archive_sha256_before="$(shasum -a 256 "$extension_archive" | awk '{ print $1 }')"
python3 - "$extension_archive" "$version" "$project_root/LICENSE" "${expected_extension_files[@]}" <<'PY'
import json
import re
import stat
import sys
import zipfile

archive_path, expected_version, license_path, *expected = sys.argv[1:]
maximum_entry_bytes = 16 * 1024 * 1024
maximum_total_bytes = 64 * 1024 * 1024
selected_payloads = {}
with zipfile.ZipFile(archive_path, "r") as archive:
    entries = archive.infolist()
    names = [item.filename for item in entries]
    if len(entries) != len(expected) or sorted(names) != sorted(expected) or len(set(names)) != len(names):
        raise SystemExit("extension archive inventory is duplicated or noncanonical")
    total = 0
    for item in entries:
        unix_mode = (item.external_attr >> 16) & 0xffff
        if (
            item.filename not in expected
            or "/" in item.filename
            or "\\" in item.filename
            or item.is_dir()
            or item.flag_bits & 0x1
            or item.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
            or item.file_size < 1
            or item.file_size > maximum_entry_bytes
            or item.compress_size < 1
            or (unix_mode and not stat.S_ISREG(unix_mode))
        ):
            raise SystemExit(f"unsafe extension archive entry: {item.filename}")
        total += item.file_size
        if total > maximum_total_bytes:
            raise SystemExit("extension archive exceeds its bounded uncompressed size")
        payload = bytearray() if item.filename in {"LICENSE", "manifest.json", "lib.js"} else None
        observed = 0
        with archive.open(item, "r") as source:
            while observed < item.file_size:
                chunk = source.read(min(1024 * 1024, item.file_size - observed))
                if not chunk:
                    raise SystemExit(f"truncated extension archive entry: {item.filename}")
                observed += len(chunk)
                if payload is not None:
                    payload.extend(chunk)
            if source.read(1):
                raise SystemExit(f"extension archive entry exceeds its declared size: {item.filename}")
        if payload is not None:
            selected_payloads[item.filename] = bytes(payload)
with open(license_path, "rb") as source:
    if selected_payloads.get("LICENSE") != source.read():
        raise SystemExit("extension archive project license differs from LICENSE")
try:
    manifest = json.loads(selected_payloads["manifest.json"].decode("utf-8"))
    library = selected_payloads["lib.js"].decode("utf-8")
except (KeyError, UnicodeDecodeError, json.JSONDecodeError) as error:
    raise SystemExit(f"extension archive metadata is invalid: {error}")
if manifest.get("version") != expected_version:
    raise SystemExit("extension archive manifest version is not candidate-bound")
matches = re.findall(r'^export const VERSION = "([^"]+)";$', library, re.MULTILINE)
if matches != [expected_version]:
    raise SystemExit("extension archive library version is not uniquely candidate-bound")
PY
test "$(shasum -a 256 "$extension_archive" | awk '{ print $1 }')" = "$extension_archive_sha256_before" \
  || { echo "Extension archive changed while it was inspected." >&2; exit 1; }

mac_stage="$(mktemp -d)"
trap 'rm -rf "$mac_stage"' EXIT
macos_archive_sha256_before="$(shasum -a 256 "$macos_archive" | awk '{ print $1 }')"
python3 - "$macos_archive" "$mac_stage" <<'PY'
import os
import stat
import sys
import tarfile

archive_path, destination = sys.argv[1:]
expected_modes = {
    "local-browser-bridge": 0o755,
    "LICENSE": 0o644,
    "THIRD_PARTY_LICENSES.txt": 0o644,
    "Local Computer Helper.app": 0o755,
    "Local Computer Helper.app/Contents": 0o755,
    "Local Computer Helper.app/Contents/Info.plist": 0o644,
    "Local Computer Helper.app/Contents/MacOS": 0o755,
    "Local Computer Helper.app/Contents/MacOS/local-computer-helper": 0o755,
    "Local Computer Helper.app/Contents/_CodeSignature": 0o755,
    "Local Computer Helper.app/Contents/_CodeSignature/CodeResources": 0o644,
    "Local Browser Bridge.app": 0o755,
    "Local Browser Bridge.app/Contents": 0o755,
    "Local Browser Bridge.app/Contents/Info.plist": 0o644,
    "Local Browser Bridge.app/Contents/MacOS": 0o755,
    "Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop": 0o755,
    "Local Browser Bridge.app/Contents/_CodeSignature": 0o755,
    "Local Browser Bridge.app/Contents/_CodeSignature/CodeResources": 0o644,
}
expected_directories = {
    "Local Computer Helper.app",
    "Local Computer Helper.app/Contents",
    "Local Computer Helper.app/Contents/MacOS",
    "Local Computer Helper.app/Contents/_CodeSignature",
    "Local Browser Bridge.app",
    "Local Browser Bridge.app/Contents",
    "Local Browser Bridge.app/Contents/MacOS",
    "Local Browser Bridge.app/Contents/_CodeSignature",
}
expected_files = set(expected_modes) - expected_directories
maximum_member_bytes = 128 * 1024 * 1024
maximum_total_bytes = 256 * 1024 * 1024
seen = set()
total = 0
with tarfile.open(archive_path, "r:gz") as bundle:
    if bundle.pax_headers:
        raise SystemExit("macOS archive contains global PAX metadata")
    for member in bundle:
        name = member.name.removesuffix("/")
        if name not in expected_modes or name in seen or member.pax_headers:
            raise SystemExit(f"macOS archive path is duplicated, unexpected, or PAX-overridden: {name}")
        seen.add(name)
        if member.mode != expected_modes[name]:
            raise SystemExit(f"macOS archive mode is noncanonical: {name}")
        output_path = os.path.join(destination, *name.split("/"))
        if name in expected_directories:
            if not member.isdir() or member.size != 0:
                raise SystemExit(f"macOS archive directory type is invalid: {name}")
            os.makedirs(output_path, mode=0o700, exist_ok=True)
            continue
        if not member.isfile() or member.issym() or member.islnk() or member.size < 1 or member.size > maximum_member_bytes:
            raise SystemExit(f"macOS archive file type or size is invalid: {name}")
        total += member.size
        if total > maximum_total_bytes:
            raise SystemExit("macOS archive exceeds its bounded uncompressed size")
        os.makedirs(os.path.dirname(output_path), mode=0o700, exist_ok=True)
        descriptor = os.open(
            output_path,
            os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
            0o600,
        )
        written = 0
        try:
            source = bundle.extractfile(member)
            if source is None:
                raise SystemExit(f"macOS archive file is unreadable: {name}")
            with source, os.fdopen(descriptor, "wb", closefd=False) as output:
                while written < member.size:
                    chunk = source.read(min(1024 * 1024, member.size - written))
                    if not chunk:
                        raise SystemExit(f"macOS archive file is truncated: {name}")
                    output.write(chunk)
                    written += len(chunk)
                if source.read(1):
                    raise SystemExit(f"macOS archive file exceeds its declared size: {name}")
                output.flush()
                os.fsync(output.fileno())
        finally:
            os.close(descriptor)
        os.chmod(output_path, expected_modes[name])
    if seen != set(expected_modes):
        raise SystemExit("macOS archive does not contain the exact canonical inventory")
PY
test "$(shasum -a 256 "$macos_archive" | awk '{ print $1 }')" = "$macos_archive_sha256_before" \
  || { echo "macOS archive changed while it was inspected." >&2; exit 1; }
mac_server="$mac_stage/local-browser-bridge"
mac_helper="$mac_stage/Local Computer Helper.app/Contents/MacOS/local-computer-helper"
mac_desktop="$mac_stage/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop"
for executable in "$mac_server" "$mac_helper" "$mac_desktop"; do
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
desktop_plist="$mac_stage/Local Browser Bridge.app/Contents/Info.plist"
if [[ ! -f "$plist" || -L "$plist" ]]; then
  echo "macOS helper metadata is not a regular file." >&2
  exit 1
fi
if [[ ! -f "$desktop_plist" || -L "$desktop_plist" ]]; then
  echo "macOS Desktop Host metadata is not a regular file." >&2
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
if [[ "$(grep -Fc "<string>$version</string>" "$desktop_plist")" -lt 2 ]] \
  || ! grep -Fq '<string>dev.flrngel.local-browser-bridge.desktop</string>' "$desktop_plist" \
  || ! grep -Fq '<key>LSUIElement</key>' "$desktop_plist" \
  || ! grep -Fq '<true/>' "$desktop_plist"; then
  echo "macOS Desktop Host metadata is missing its version, bundle identity, or menu-bar policy." >&2
  exit 1
fi

if [[ "$(uname -s)" == "Darwin" ]]; then
  macos_verifier_arguments=("$version" "$mac_server" "$mac_helper" "$mac_desktop")
  if [[ "$verification_mode" == "static-only" ]]; then
    macos_verifier_arguments+=("--static-only")
  fi
  bash scripts/verify-macos-artifacts.sh "${macos_verifier_arguments[@]}"
fi

if [[ ! -f "$checksum_manifest" || -L "$checksum_manifest" || ! -s "$checksum_manifest" ]]; then
  echo "Release checksum manifest is missing, empty, linked, or not a regular file: $checksum_manifest" >&2
  exit 1
fi
expected_checksum_files=(
  "$(basename "$windows_server")"
  "$(basename "$windows_helper")"
  "$(basename "$macos_archive")"
  "$(basename "$extension_archive")"
)
checksum_lines=()
while IFS= read -r line || [[ -n "$line" ]]; do
  checksum_lines+=("$line")
done < "$checksum_manifest"
if (( ${#checksum_lines[@]} != ${#expected_checksum_files[@]} )); then
  echo "Checksum manifest must contain exactly four canonical lines." >&2
  exit 1
fi
canonical_checksum_manifest="$mac_stage/canonical-SHA256SUMS.txt"
: > "$canonical_checksum_manifest"
for index in "${!expected_checksum_files[@]}"; do
  line="${checksum_lines[$index]}"
  hash="${line:0:64}"
  if [[ ! "$hash" =~ ^[0-9a-f]{64}$ ]] \
    || [[ "${line:64:2}" != "  " ]] \
    || [[ "${line:66}" != "${expected_checksum_files[$index]}" ]]; then
    echo "Checksum manifest is not canonical at line $((index + 1))." >&2
    exit 1
  fi
  printf '%s  %s\n' "$hash" "${expected_checksum_files[$index]}" >> "$canonical_checksum_manifest"
done
if ! cmp -s "$canonical_checksum_manifest" "$checksum_manifest"; then
  echo "Checksum manifest bytes are not canonical LF-terminated ASCII." >&2
  exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$assets_dir" && sha256sum --check SHA256SUMS.txt)
else
  (cd "$assets_dir" && shasum -a 256 -c SHA256SUMS.txt)
fi

echo "Verified release assets for $version."
