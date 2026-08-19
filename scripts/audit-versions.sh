#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

cargo_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)"
lock_version="$(awk '
  $0 == "[[package]]" { matching = 0 }
  $0 == "name = \"local-browser-bridge\"" { matching = 1; next }
  matching && /^version = "/ {
    value = $0
    sub(/^version = "/, "", value)
    sub(/"$/, "", value)
    print value
    exit
  }
' Cargo.lock)"
manifest_version="$(sed -n 's/.*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' extension/manifest.json | head -n 1)"
library_version="$(sed -n 's/^export const VERSION = "\([^"]*\)";$/\1/p' extension/lib.js | head -n 1)"

if [[ -z "$cargo_version" || -z "$lock_version" || -z "$manifest_version" || -z "$library_version" ]]; then
  echo "Could not read every distributed version." >&2
  exit 1
fi

if [[ "$cargo_version" != "$lock_version" || "$cargo_version" != "$manifest_version" || "$cargo_version" != "$library_version" ]]; then
  printf '%s\n' \
    "Distributed versions are not aligned:" \
    "  Cargo.toml:             $cargo_version" \
    "  Cargo.lock:             $lock_version" \
    "  extension/manifest.json: $manifest_version" \
    "  extension/lib.js:        $library_version" >&2
  exit 1
fi

expected="${1:-}"
expected="${expected#v}"
if [[ -n "$expected" && "$cargo_version" != "$expected" ]]; then
  echo "Expected version $expected, found $cargo_version." >&2
  exit 1
fi

if [[ ! "$cargo_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]]; then
  echo "Version is not a supported release version: $cargo_version" >&2
  exit 1
fi

echo "$cargo_version"
