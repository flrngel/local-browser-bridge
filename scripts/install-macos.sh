#!/usr/bin/env bash
set -euo pipefail

repository="flrngel/local-browser-bridge"
version="latest"
install_root="$HOME/Applications/Local Browser Bridge"
startup=1
start_helper=0
launch=1
uninstall=0
reset_token=0
self_test=0

usage() {
  cat <<'EOF'
Usage: install-macos.sh [--version latest|VERSION] [--install-root PATH]
                        [--no-startup] [--start-helper] [--no-launch]
                        [--uninstall] [--reset-token] [--self-test]
EOF
}

while (($#)); do
  case "$1" in
    --version) version="${2:?missing version}"; shift 2 ;;
    --install-root) install_root="${2:?missing install root}"; shift 2 ;;
    --no-startup) startup=0; shift ;;
    --start-helper) start_helper=1; shift ;;
    --no-launch) launch=0; shift ;;
    --uninstall) uninstall=1; shift ;;
    --reset-token) reset_token=1; shift ;;
    --self-test) self_test=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

sha256() { /usr/bin/shasum -a 256 "$1" | /usr/bin/awk '{print $1}'; }
assert_ordinary_dir() {
  local path=$1
  [[ -d "$path" ]] || { echo "Required directory does not exist: $path" >&2; exit 1; }
  [[ ! -L "$path" ]] || { echo "Refusing a symlink install path: $path" >&2; exit 1; }
}

assert_safe_install_root() {
  case "$install_root" in
    "$HOME"/*) ;;
    *) echo "Install root must be a child of the current user's home directory." >&2; exit 1 ;;
  esac
  [[ "$install_root" != "$HOME" && "$install_root" != / ]] || { echo "Refusing a broad install root." >&2; exit 1; }
  local cursor
  cursor="$(/usr/bin/dirname "$install_root")"
  while [[ "$cursor" != "$HOME" && "$cursor" != / ]]; do
    if [[ -e "$cursor" ]]; then assert_ordinary_dir "$cursor"; fi
    cursor="$(/usr/bin/dirname "$cursor")"
  done
}

manifest_value() {
  local manifest=$1 name=$2
  /usr/bin/awk -v target="$name" '
    BEGIN { found=0 }
    $0 ~ /^[0-9a-f]{64}  [A-Za-z0-9._-]+$/ && $2 == target { print $1; found++ }
    END { if (found != 1) exit 1 }
  ' "$manifest"
}

parse_release() {
  local json=$1 output=$2
  /usr/bin/osascript -l JavaScript - "$json" "$output" <<'JXA'
ObjC.import('Foundation');
function run(args) {
  const data = $.NSData.dataWithContentsOfFile(args[0]);
  if (!data) throw new Error('release JSON could not be read');
  const obj = JSON.parse($.NSString.alloc.initWithDataEncoding(data, $.NSUTF8StringEncoding).js);
  if (obj.draft || obj.prerelease || obj.immutable !== true || !/^v\d+\.\d+\.\d+$/.test(obj.tag_name)) {
    throw new Error('release is not canonical, stable, and immutable');
  }
  const version = obj.tag_name.slice(1);
  const expected = [
    `local-browser-bridge-extension-v${version}.zip`,
    `local-browser-bridge-v${version}-macos-universal.tar.gz`,
    `local-browser-bridge-v${version}-windows-x86_64.exe`,
    `local-computer-helper-v${version}-windows-x86_64.exe`,
    'SHA256SUMS.txt'
  ];
  if (!Array.isArray(obj.assets) || obj.assets.length !== expected.length) throw new Error('unexpected asset count');
  const lines = [`VERSION\t${version}`];
  for (const name of expected) {
    const matches = obj.assets.filter(a => a.name === name);
    if (matches.length !== 1 || !(matches[0].size > 0) || matches[0].state !== 'uploaded' ||
        !/^sha256:[0-9a-f]{64}$/.test(matches[0].digest) || !/^https:\/\/github\.com\//.test(matches[0].browser_download_url)) {
      throw new Error(`missing or unverifiable asset: ${name}`);
    }
    lines.push(`${name}\t${matches[0].digest.slice(7)}\t${matches[0].browser_download_url}`);
  }
  $(lines.join('\n') + '\n').writeToFileAtomicallyEncodingError(args[1], true, $.NSUTF8StringEncoding, null);
}
JXA
}

label="dev.flrngel.local-browser-bridge"
plist="$HOME/Library/LaunchAgents/$label.plist"
assert_safe_install_root

if ((self_test)); then
  scratch="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/lbb-installer-self-test.XXXXXX")"
  trap '/bin/rm -rf -- "$scratch"' EXIT
  printf '%064d  file1.bin\n%064d  file2.bin\n%064d  file3.bin\n%064d  file4.bin\n' 0 0 0 0 > "$scratch/SHA256SUMS.txt"
  [[ "$(manifest_value "$scratch/SHA256SUMS.txt" file4.bin)" == "$(printf '%064d' 0)" ]]
  printf '%s\n' 'macOS one-command installer self-test passed.'
  exit 0
fi

if ((uninstall)); then
  /bin/launchctl bootout "gui/$(/usr/bin/id -u)/$label" >/dev/null 2>&1 || true
  [[ ! -e "$plist" || -f "$plist" ]] || { echo "Refusing a non-file LaunchAgent path." >&2; exit 1; }
  /bin/rm -f -- "$plist"
  if [[ -e "$install_root" ]]; then
    assert_ordinary_dir "$install_root"
    /bin/rm -rf -- "$install_root"
  fi
  ((reset_token)) && /bin/rm -f -- "$HOME/.local-browser-bridge/token"
  echo "Local Browser Bridge was removed for the current user."
  exit 0
fi

[[ "$(/usr/bin/sw_vers -productVersion | /usr/bin/awk -F. '{print $1}')" -ge 13 ]] || { echo "macOS 13 or later is required." >&2; exit 1; }
if [[ "$version" != latest && ! "$version" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version must be 'latest' or a stable semantic version." >&2; exit 2
fi

stage="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/lbb-install.XXXXXX")"
trap '/bin/rm -rf -- "$stage"' EXIT
api="https://api.github.com/repos/$repository/releases/latest"
[[ "$version" == latest ]] || api="https://api.github.com/repos/$repository/releases/tags/v${version#v}"
/usr/bin/curl --fail --silent --show-error --location --header 'Accept: application/vnd.github+json' --user-agent local-browser-bridge-installer "$api" --output "$stage/release.json"
parse_release "$stage/release.json" "$stage/release.tsv"
resolved="$(/usr/bin/awk -F '\t' '$1 == "VERSION" { print $2 }' "$stage/release.tsv")"
[[ "$resolved" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Release version parsing failed." >&2; exit 1; }
archive="local-browser-bridge-v$resolved-macos-universal.tar.gz"
extension_zip="local-browser-bridge-extension-v$resolved.zip"

for name in "$archive" "$extension_zip" SHA256SUMS.txt; do
  expected="$(/usr/bin/awk -F '\t' -v n="$name" '$1 == n { print $2 }' "$stage/release.tsv")"
  url="$(/usr/bin/awk -F '\t' -v n="$name" '$1 == n { print $3 }' "$stage/release.tsv")"
  [[ "$expected" =~ ^[0-9a-f]{64}$ && "$url" == https://github.com/* ]] || { echo "Missing trusted metadata for $name" >&2; exit 1; }
  /usr/bin/curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 "$url" --output "$stage/$name"
  [[ "$(sha256 "$stage/$name")" == "$expected" ]] || { echo "GitHub digest mismatch for $name" >&2; exit 1; }
done

[[ "$(/usr/bin/wc -l < "$stage/SHA256SUMS.txt" | /usr/bin/tr -d ' ')" == 4 ]] || { echo "The checksum manifest is not canonical." >&2; exit 1; }
for name in "$archive" "$extension_zip"; do
  [[ "$(manifest_value "$stage/SHA256SUMS.txt" "$name")" == "$(sha256 "$stage/$name")" ]] || { echo "Manifest digest mismatch for $name" >&2; exit 1; }
done

/usr/bin/tar -xzf "$stage/$archive" -C "$stage"
/usr/bin/ditto -x -k "$stage/$extension_zip" "$stage/extension"
[[ -x "$stage/local-browser-bridge" && -d "$stage/Local Computer Helper.app" && -f "$stage/extension/manifest.json" ]] || { echo "A package has an unexpected layout." >&2; exit 1; }

parent="$(/usr/bin/dirname "$install_root")"
/bin/mkdir -p -- "$parent"
assert_ordinary_dir "$parent"
if [[ -e "$install_root" ]]; then assert_ordinary_dir "$install_root"; /bin/rm -rf -- "$install_root"; fi
/bin/mkdir -p -- "$install_root"
/bin/cp "$stage/local-browser-bridge" "$install_root/local-browser-bridge"
/usr/bin/ditto "$stage/Local Computer Helper.app" "$install_root/Local Computer Helper.app"
/usr/bin/ditto "$stage/extension" "$install_root/extension"
/bin/cp "$stage/SHA256SUMS.txt" "$install_root/SHA256SUMS.txt"

if ((startup)); then
  /bin/mkdir -p -- "$HOME/Library/LaunchAgents"
  escaped_root="$(printf '%s' "$install_root/local-browser-bridge" | /usr/bin/sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')"
  /bin/cat > "$stage/launchagent.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$label</string>
<key>ProgramArguments</key><array><string>$escaped_root</string></array>
<key>RunAtLoad</key><true/><key>KeepAlive</key><true/>
</dict></plist>
EOF
  /usr/bin/plutil -lint "$stage/launchagent.plist" >/dev/null
  /bin/launchctl bootout "gui/$(/usr/bin/id -u)/$label" >/dev/null 2>&1 || true
  /bin/cp "$stage/launchagent.plist" "$plist"
  /bin/launchctl bootstrap "gui/$(/usr/bin/id -u)" "$plist"
else
  /bin/launchctl bootout "gui/$(/usr/bin/id -u)/$label" >/dev/null 2>&1 || true
  /bin/rm -f -- "$plist"
fi

if ((launch)); then
  if ((!startup)); then "$install_root/local-browser-bridge" >/dev/null 2>&1 & fi
  if ((start_helper)); then /usr/bin/open "$install_root/Local Computer Helper.app"; fi
  token_path="$HOME/.local-browser-bridge/token"
  for _ in {1..100}; do [[ -f "$token_path" ]] && break; /bin/sleep 0.1; done
  if [[ -f "$token_path" ]]; then
    token="$(/bin/cat "$token_path")"
    [[ "$token" =~ ^[A-Za-z0-9_-]{32,}$ ]] && /usr/bin/open "http://127.0.0.1:17373/#token=$token"
  fi
  if [[ -d '/Applications/Google Chrome.app' ]]; then
    /usr/bin/open -a 'Google Chrome' 'chrome://extensions'
  elif [[ -d '/Applications/Microsoft Edge.app' ]]; then
    /usr/bin/open -a 'Microsoft Edge' 'edge://extensions'
  fi
fi

echo "Installed Local Browser Bridge $resolved for the current user."
echo "Extension folder: $install_root/extension"
echo "In chrome://extensions, enable Developer mode, choose Load unpacked, and select that folder."
((start_helper)) || echo "Desktop control remains off. Open Local Computer Helper.app only when you want it."
