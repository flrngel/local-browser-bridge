#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
server_path="${2:-}"
helper_path="${3:-}"
if [[ -z "$version" || -z "$server_path" || -z "$helper_path" ]]; then
  echo "Usage: $0 VERSION SERVER_PATH HELPER_PATH" >&2
  exit 1
fi
version="${version#v}"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS artifact inspection requires a macOS host." >&2
  exit 1
fi
for command in lipo otool codesign nm strings; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required macOS inspection command is unavailable: $command" >&2
    exit 1
  fi
done

forbidden_helper_apis=(
  CGWarpMouseCursorPosition
  CGDisplayMoveCursorToPoint
  CGAssociateMouseAndMouseCursorPosition
  CGDisplayHideCursor
  CGDisplayShowCursor
  CGEventPost
  CGEventTapPostEvent
  CGPostMouseEvent
  CGPostScrollWheelEvent
  CGPostKeyboardEvent
  IOHIDPostEvent
  IOHIDSetCursorEnable
  IOHIDSetCursorPosition
  CGEventPostToPSN
  CGEventPostToPid
)

allowed_dynamic_lookup_symbols=(
  CGEventSetWindowLocation
  CGSGetActiveSpace
  CGSMainConnectionID
  GetProcessPID
  SLEventPostToPid
  SLEventSetIntegerValueField
  SLPSPostEventRecordTo
  SLSGetActiveSpace
  SLSGetConnectionPSN
  SLSGetWindowOwner
  _SLPSGetFrontProcess
)

api_audit_directory=""
cleanup_api_audit() {
  if [[ -n "$api_audit_directory" ]]; then
    rm -rf -- "$api_audit_directory"
  fi
}
trap cleanup_api_audit EXIT

report_mentions_api() {
  local api="$1"
  shift
  local pattern="(^|[^[:alnum:]_])_?${api}([^[:alnum:]_]|$)"
  LC_ALL=C grep -Eq "$pattern" "$@"
}

audit_helper_api_slice() {
  local architecture="$1"
  local slice_path="$api_audit_directory/local-computer-helper-$architecture"
  local undefined_report="$slice_path.nm-u"
  local strings_report="$slice_path.strings"

  lipo -thin "$architecture" "$helper_path" -output "$slice_path"
  if [[ "$(lipo -archs "$slice_path")" != "$architecture" ]]; then
    echo "macOS helper API audit did not produce one exact $architecture slice." >&2
    exit 1
  fi
  nm -u "$slice_path" > "$undefined_report"
  strings -a "$slice_path" > "$strings_report"

  for api in "${forbidden_helper_apis[@]}"; do
    if report_mentions_api "$api" "$undefined_report" "$strings_report"; then
      echo "macOS $architecture helper slice contains forbidden global input API: $api" >&2
      exit 1
    fi
  done

  for api in SLEventPostToPid CGEventSetWindowLocation; do
    if ! LC_ALL=C grep -Fxq "$api" "$strings_report"; then
      echo "macOS $architecture helper slice is missing required target-routed API: $api" >&2
      exit 1
    fi
  done
  if ! LC_ALL=C grep -Eq '(^|[[:space:]])_dlsym$' "$undefined_report"; then
    echo "macOS $architecture helper slice is missing its bounded dynamic resolver." >&2
    exit 1
  fi

  local dynamic_lookup_pattern='^(SLEvent[A-Za-z0-9_]*|SLPS[A-Za-z0-9_]*|_SLPS[A-Za-z0-9_]*|SLS(Get|Set|Copy|Create|Main|Register|Unregister)[A-Za-z0-9_]*|CGS(Get|Set|Copy|Create|Main|Register|Unregister)[A-Za-z0-9_]*|GetProcessPID|CGEventSetWindowLocation)$'
  local observed_dynamic_symbols
  local expected_dynamic_symbols
  observed_dynamic_symbols="$({ LC_ALL=C grep -E "$dynamic_lookup_pattern" "$strings_report" || true; } | LC_ALL=C sort -u)"
  expected_dynamic_symbols="$(printf '%s\n' "${allowed_dynamic_lookup_symbols[@]}" | LC_ALL=C sort)"
  if [[ "$observed_dynamic_symbols" != "$expected_dynamic_symbols" ]]; then
    echo "macOS $architecture helper slice has a missing or unreviewed dynamic lookup symbol." >&2
    exit 1
  fi
}

deployment_target_for_slice() {
  local executable="$1"
  local architecture="$2"
  otool -l -arch "$architecture" "$executable" | awk '
    $1 == "cmd" {
      build = ($2 == "LC_BUILD_VERSION")
      legacy = ($2 == "LC_VERSION_MIN_MACOSX")
      next
    }
    build && $1 == "minos" { print $2; exit }
    legacy && $1 == "version" { print $2; exit }
  '
}

for executable in "$server_path" "$helper_path"; do
  if [[ ! -f "$executable" || -L "$executable" || ! -x "$executable" ]]; then
    echo "macOS executable is missing, linked, or not executable: $executable" >&2
    exit 1
  fi

  architectures="$(lipo -archs "$executable")"
  for architecture in arm64 x86_64; do
    if ! grep -Eq "(^|[[:space:]])${architecture}([[:space:]]|$)" <<<"$architectures"; then
      echo "macOS universal binary is missing $architecture: $executable ($architectures)" >&2
      exit 1
    fi
    deployment_target="$(deployment_target_for_slice "$executable" "$architecture")"
    if [[ "$deployment_target" != "13.0" ]]; then
      echo "macOS $architecture slice has deployment target '$deployment_target', expected 13.0: $executable" >&2
      exit 1
    fi
  done
  if [[ "$(wc -w <<<"$architectures" | tr -d ' ')" != "2" ]]; then
    echo "macOS universal binary contains an unexpected architecture: $executable ($architectures)" >&2
    exit 1
  fi

  codesign --verify --strict "$executable"
done

api_audit_directory="$(mktemp -d "${TMPDIR:-/tmp}/lbb-macos-helper-api.XXXXXX")"
for architecture in arm64 x86_64; do
  audit_helper_api_slice "$architecture"
done

if [[ "$("$server_path" --version)" != "local-browser-bridge $version" ]]; then
  echo "macOS server version does not match $version." >&2
  exit 1
fi
if [[ "$("$helper_path" --version)" != "local-computer-helper $version" ]]; then
  echo "macOS helper version does not match $version." >&2
  exit 1
fi

for executable in "$server_path" "$helper_path"; do
  license_report="$("$executable" --licenses)"
  grep -Fq 'Local Browser Bridge third-party licenses' <<<"$license_report"
  grep -Fq 'MIT License' <<<"$license_report"
  grep -Fq 'Apache License' <<<"$license_report"
done

echo "Verified macOS universal artifacts for $version."
