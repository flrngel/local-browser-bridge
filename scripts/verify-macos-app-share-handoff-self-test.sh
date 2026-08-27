#!/usr/bin/env bash
set -euo pipefail

if (( $# < 1 || $# > 2 )); then
  echo "Usage: scripts/verify-macos-app-share-handoff-self-test.sh VERSION [--historical-source]" >&2
  exit 2
fi

version="$1"
historical_source=0
if (( $# == 2 )); then
  if [[ "$2" != "--historical-source" ]]; then
    echo "Usage: scripts/verify-macos-app-share-handoff-self-test.sh VERSION [--historical-source]" >&2
    exit 2
  fi
  historical_source=1
fi
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "The app-share handoff self-test requires a canonical package version." >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$repo_root"
if (( historical_source == 0 )); then
  test "$(bash scripts/audit-versions.sh "$version")" = "$version"
fi

source_path="evidence/v${version}/computer/AppShareHandoff.swift"
if [[ ! -f "$source_path" || -L "$source_path" ]]; then
  echo "The version-bound app-share handoff source is missing or unsafe." >&2
  exit 1
fi

scratch="$(mktemp -d "${TMPDIR:-/tmp}/lbb-app-share-self-test.XXXXXX")"
binary="$scratch/lbb-app-share-handoff-self-test"
stdout_path="$scratch/stdout"
stderr_path="$scratch/stderr"
expected_path="$scratch/expected"

cleanup() {
  rm -f "$binary" "$stdout_path" "$stderr_path" "$expected_path"
  rmdir "$scratch" 2>/dev/null || true
}
trap cleanup EXIT

xcrun swiftc -typecheck "$source_path"
xcrun swiftc "$source_path" -o "$binary"

set +e
"$binary" --self-test >"$stdout_path" 2>"$stderr_path"
status=$?
set -e

printf '%s\n' 'macOS app-share handoff self-test passed' > "$expected_path"
if (( status != 0 )) || [[ -s "$stderr_path" ]] || ! cmp -s "$stdout_path" "$expected_path"; then
  echo "macOS app-share handoff self-test output contract failed." >&2
  printf 'exit=%s stdout_bytes=%s stderr_bytes=%s\n' \
    "$status" \
    "$(wc -c < "$stdout_path" | tr -d '[:space:]')" \
    "$(wc -c < "$stderr_path" | tr -d '[:space:]')" >&2
  exit 1
fi

printf '%s\n' 'macOS app-share handoff self-test passed'
