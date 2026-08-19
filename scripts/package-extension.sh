#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

version="$(bash scripts/audit-versions.sh)"

output="${1:-$project_root/dist/local-browser-bridge-extension-v${version}.zip}"
mkdir -p "$(dirname "$output")"
output_dir="$(cd "$(dirname "$output")" && pwd -P)"
output="$output_dir/$(basename "$output")"
rm -f "$output"

files=(background.js content.js lib.js manifest.json popup.css popup.html popup.js)
for file in "${files[@]}"; do
  if [[ ! -f "extension/$file" || -L "extension/$file" ]]; then
    echo "Missing extension package file: $file" >&2
    exit 1
  fi
done

stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
for file in "${files[@]}"; do
  cp "extension/$file" "$stage/$file"
  chmod 644 "$stage/$file"
  touch -t 198001010000.00 "$stage/$file"
done

(
  cd "$stage"
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

for file in "${files[@]}"; do
  if ! unzip -p "$output" "$file" | cmp -s - "extension/$file"; then
    echo "Extension archive payload differs from extension/$file." >&2
    exit 1
  fi
done

echo "$output"
