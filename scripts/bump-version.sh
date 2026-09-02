#!/usr/bin/env bash
# Rewrites every pinned distributed-version literal in one command.
#
# Usage:
#   bash scripts/bump-version.sh NEW_VERSION
#   bash scripts/bump-version.sh --self-test
#
# The pinned locations live in scripts/version-pins.txt and are shared with
# scripts/audit-versions.sh, so a location that is added there is both
# rewritten here and audited before every release.
set -euo pipefail
export LC_ALL=C

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

pins_file="$project_root/scripts/version-pins.txt"

escape_bre() {
  printf '%s' "$1" | sed 's/[][\.*^$/]/\\&/g'
}

escape_replacement() {
  printf '%s' "$1" | sed 's/[&/\]/\\&/g'
}

# rewrite_pins OLD NEW ROOT PINS_FILE
rewrite_pins() {
  local old="$1" new="$2" root="$3" pins="$4"
  local file prefix suffix scope anchor search replacement
  while IFS='|' read -r file prefix suffix scope; do
    [[ -n "$file" && "$file" != \#* ]] || continue
    anchor=""
    [[ "$scope" == "line" ]] && anchor="^"
    search="$anchor$(escape_bre "$prefix$old$suffix")"
    replacement="$(escape_replacement "$prefix$new$suffix")"
    if [[ ! -f "$root/$file" ]] || ! grep -q -- "$search" "$root/$file"; then
      echo "Pinned version literal not found in $file: $prefix$old$suffix" >&2
      return 1
    fi
    sed -i.bump-bak "s/$search/$replacement/g" "$root/$file"
    rm -f "$root/$file.bump-bak"
  done < "$pins"
}

# rewrite_lock NEW ROOT
rewrite_lock() {
  local new="$1" root="$2"
  awk -v new="$new" '
    $0 == "[[package]]" { matching = 0 }
    $0 == "name = \"local-browser-bridge\"" { matching = 1; print; next }
    matching && /^version = "/ { print "version = \"" new "\""; matching = 0; next }
    { print }
  ' "$root/Cargo.lock" > "$root/Cargo.lock.bump"
  mv "$root/Cargo.lock.bump" "$root/Cargo.lock"
}

self_test() {
  local scratch
  scratch="$(mktemp -d)"
  mkdir -p "$scratch/extension" "$scratch/.github/workflows" "$scratch/docs"
  printf '[package]\nname = "local-browser-bridge"\nversion = "1.2.3"\n\n[dependencies]\nother = { version = "1.2.3" }\n' > "$scratch/Cargo.toml"
  printf '[[package]]\nname = "other"\nversion = "1.2.3"\n\n[[package]]\nname = "local-browser-bridge"\nversion = "1.2.3"\n' > "$scratch/Cargo.lock"
  printf '{\n  "version": "1.2.3",\n  "minimum_chrome_version": "140"\n}\n' > "$scratch/extension/manifest.json"
  printf 'export const VERSION = "1.2.3";\n' > "$scratch/extension/lib.js"
  printf 'description: Exact package version from Cargo.toml (for example, 1.2.3)\n' > "$scratch/.github/workflows/deploy.yml"
  printf 'description: Exact accepted package version (for example, 1.2.3)\n' > "$scratch/.github/workflows/publish.yml"
  printf 'VERSION="1.2.3"\nother 1.2.3 text\n' > "$scratch/docs/DEVELOPMENT.md"

  rewrite_pins 1.2.3 1.2.4 "$scratch" "$pins_file"
  rewrite_lock 1.2.4 "$scratch"
  grep -q '^version = "1.2.4"$' "$scratch/Cargo.toml"
  grep -q 'other = { version = "1.2.3" }' "$scratch/Cargo.toml"
  test "$(grep -c '^version = "1.2.4"$' "$scratch/Cargo.lock")" = 1
  test "$(grep -c '^version = "1.2.3"$' "$scratch/Cargo.lock")" = 1
  grep -q '"version": "1.2.4"' "$scratch/extension/manifest.json"
  grep -q '"minimum_chrome_version": "140"' "$scratch/extension/manifest.json"
  grep -q '^export const VERSION = "1.2.4";$' "$scratch/extension/lib.js"
  grep -q '(for example, 1.2.4)' "$scratch/.github/workflows/deploy.yml"
  grep -q '(for example, 1.2.4)' "$scratch/.github/workflows/publish.yml"
  grep -q '^VERSION="1.2.4"$' "$scratch/docs/DEVELOPMENT.md"
  grep -q '^other 1.2.3 text$' "$scratch/docs/DEVELOPMENT.md"
  if rewrite_pins 9.9.9 1.2.5 "$scratch" "$pins_file" 2>/dev/null; then
    echo "bump-version self-test: a missing pin must fail" >&2
    rm -rf "$scratch"
    return 1
  fi
  rm -rf "$scratch"
  echo "bump-version self-test passed."
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
  exit 0
fi

new_version="${1:-}"
new_version="${new_version#v}"
if [[ ! "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Usage: bash scripts/bump-version.sh NEW_VERSION (for example, 1.2.3)" >&2
  exit 2
fi

current_version="$(bash scripts/audit-versions.sh)"
if [[ "$current_version" == "$new_version" ]]; then
  echo "Version is already $new_version." >&2
  exit 1
fi

rewrite_pins "$current_version" "$new_version" "$project_root" "$pins_file"
rewrite_lock "$new_version" "$project_root"
cargo metadata --locked --offline --format-version 1 >/dev/null
bash scripts/audit-versions.sh "$new_version" >/dev/null
echo "Bumped $current_version -> $new_version (Cargo.lock plus every pin in scripts/version-pins.txt)."
