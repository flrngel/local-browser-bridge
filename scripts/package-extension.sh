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

output="${1:-$project_root/dist/local-browser-bridge-extension-v${version}.zip}"
mkdir -p "$(dirname "$output")"
rm -f "$output"

files=(background.js content.js lib.js manifest.json popup.css popup.html popup.js)
for file in "${files[@]}"; do
  if [[ ! -f "extension/$file" ]]; then
    echo "Missing extension package file: $file" >&2
    exit 1
  fi
done

(
  cd extension
  zip -q -X "$output" "${files[@]}"
)
unzip -tq "$output" >/dev/null

expected="$(printf '%s\n' "${files[@]}" | LC_ALL=C sort)"
actual="$(unzip -Z1 "$output" | LC_ALL=C sort)"
if [[ "$actual" != "$expected" ]]; then
  echo "Extension archive contains an unexpected file set." >&2
  diff -u <(printf '%s\n' "$expected") <(printf '%s\n' "$actual") || true
  exit 1
fi

echo "$output"
