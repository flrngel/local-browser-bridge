#!/usr/bin/env bash
set -euo pipefail

required_sdk_major=26
required_deployment_target="13.0"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS build-host verification requires macOS." >&2
  exit 1
fi

for command in xcrun swift; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required macOS build command is unavailable: $command" >&2
    exit 1
  fi
done

sdk_version="$(xcrun --sdk macosx --show-sdk-version)"
if [[ ! "$sdk_version" =~ ^([0-9]+)(\.[0-9]+)*$ ]]; then
  echo "Could not parse the active macOS SDK version: $sdk_version" >&2
  exit 1
fi
sdk_major="${BASH_REMATCH[1]}"
if (( sdk_major < required_sdk_major )); then
  echo "macOS SDK $required_sdk_major or newer is required; active SDK is $sdk_version." >&2
  echo "The locked apple-metal Swift bridge names Metal APIs introduced in the macOS 26 SDK." >&2
  exit 1
fi

deployment_target="$(awk -F '"' '
  /^MACOSX_DEPLOYMENT_TARGET = / {
    count += 1
    value = $2
  }
  END {
    if (count != 1) {
      exit 1
    }
    print value
  }
' "$repo_root/.cargo/config.toml")"
if [[ "$deployment_target" != "$required_deployment_target" ]]; then
  echo "Configured macOS deployment target is '$deployment_target'; expected $required_deployment_target." >&2
  exit 1
fi

echo "Verified macOS build host: SDK $sdk_version; deployment target $deployment_target."
