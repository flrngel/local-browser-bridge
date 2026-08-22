#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

expected_version="cargo-about 0.9.2"
if [[ "$(cargo about --version)" != "$expected_version" ]]; then
  echo "$expected_version with the cli feature is required to verify dependency licenses." >&2
  exit 1
fi

mode="${1:-check}"
if [[ "$mode" != check && "$mode" != --write ]]; then
  echo "Usage: $0 [--write]" >&2
  exit 1
fi

raw="$(mktemp)"
generated="$(mktemp)"
trap 'find "$raw" "$generated" -delete' EXIT
cargo about generate about.hbs --locked --fail --output-file "$raw"
LC_ALL=C awk '
  {
    sub(/[[:space:]]+$/, "")
    lines[NR] = $0
    if ($0 != "") last = NR
  }
  END {
    for (line = 1; line <= last; line++) print lines[line]
  }
' "$raw" >"$generated"

if [[ "$mode" == --write ]]; then
  cp "$generated" THIRD_PARTY_LICENSES.txt
else
  cmp "$generated" THIRD_PARTY_LICENSES.txt
fi

for forbidden in option-ext 'Mozilla Public License' '/Users/' '\Users\'; do
  if grep -Fq "$forbidden" THIRD_PARTY_LICENSES.txt; then
    echo "Generated license report contains forbidden or host-specific text: $forbidden" >&2
    exit 1
  fi
done

grep -Fq 'Apache License' THIRD_PARTY_LICENSES.txt
grep -Fq 'MIT License' THIRD_PARTY_LICENSES.txt
grep -Fq 'Unicode License' THIRD_PARTY_LICENSES.txt
echo "Verified locked third-party license report."
