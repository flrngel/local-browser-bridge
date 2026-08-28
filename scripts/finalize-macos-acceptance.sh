#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
FINALIZER="$SCRIPT_DIR/finalize-macos-acceptance.mjs"

fail() {
  printf 'macOS acceptance finalizer wrapper failed: %s\n' "$1" >&2
  exit 1
}

create_aggregate_directory() {
  local version="$1"
  local private_parent="$2"
  local aggregate_directory
  local aggregate_canonical

  [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
    fail "version must be a dotted numeric package version"
  [[ -d "$private_parent" && ! -L "$private_parent" ]] ||
    fail "private parent must be an existing ordinary directory"

  private_parent="$(cd "$private_parent" && pwd -P)"
  [[ "$(stat -f '%HT:%Lp:%u' "$private_parent")" == \
    "Directory:700:$(id -u)" ]] ||
    fail "private parent must be owner-private and current-user owned"

  aggregate_directory="$(
    mktemp -d "$private_parent/lbb-v${version}-aggregate.XXXXXX"
  )"
  aggregate_canonical="$(cd "$aggregate_directory" && pwd -P)"
  [[ "$aggregate_canonical" == "$aggregate_directory" ]] ||
    fail "aggregate directory must already use its canonical path"
  [[ "$(stat -f '%HT:%Lp:%u' "$aggregate_canonical")" == \
    "Directory:700:$(id -u)" ]] ||
    fail "aggregate directory must be owner-private and current-user owned"
  [[ -z "$(find "$aggregate_canonical" -mindepth 1 -print -quit)" ]] ||
    fail "aggregate directory must start empty"

  printf '%s\n' "$aggregate_canonical"
}

if [[ "${1:-}" == "--self-test" ]]; then
  SELF_TEST_PARENT="$(mktemp -d)"
  chmod 700 "$SELF_TEST_PARENT"
  SELF_TEST_PARENT="$(cd "$SELF_TEST_PARENT" && pwd -P)"
  trap 'rm -rf -- "$SELF_TEST_PARENT"' EXIT
  SELF_TEST_OUTPUT="$(create_aggregate_directory 9.8.7 "$SELF_TEST_PARENT")"
  [[ "$SELF_TEST_OUTPUT" == "$SELF_TEST_PARENT/lbb-v9.8.7-aggregate."* ]] ||
    fail "self-test aggregate path is not version-bound"
  [[ -d "$SELF_TEST_OUTPUT" && ! -L "$SELF_TEST_OUTPUT" ]] ||
    fail "self-test aggregate path is not an ordinary directory"
  printf '%s\n' 'macOS acceptance finalizer wrapper self-test passed.'
  exit 0
fi

[[ "$#" == 4 ]] || {
  printf '%s\n' \
    'usage: finalize-macos-acceptance.sh VERSION QUIET_DIR DELIBERATE_DIR PRIVATE_PARENT' >&2
  exit 2
}

VERSION="$1"
QUIET_DIRECTORY="$2"
DELIBERATE_DIRECTORY="$3"
PRIVATE_PARENT="$4"

[[ -f "$FINALIZER" && ! -L "$FINALIZER" ]] ||
  fail "checked-in JavaScript finalizer is unavailable"

AGGREGATE_CANONICAL="$(
  create_aggregate_directory "$VERSION" "$PRIVATE_PARENT"
)"

# The JavaScript finalizer writes progress to stdout. Keep stdout reserved for
# the one canonical aggregate path so callers can bind it atomically without
# combining dependent shell assignments.
node "$FINALIZER" \
  "$QUIET_DIRECTORY" \
  "$DELIBERATE_DIRECTORY" \
  "$AGGREGATE_CANONICAL" >&2

[[ -f "$AGGREGATE_CANONICAL/macos-acceptance.json" ]] ||
  fail "finalizer did not create the aggregate result"
printf '%s\n' "$AGGREGATE_CANONICAL"
