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
for command in lipo otool codesign; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required macOS inspection command is unavailable: $command" >&2
    exit 1
  fi
done

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
