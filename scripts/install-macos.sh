#!/usr/bin/env bash
set -euo pipefail

repository="flrngel/local-browser-bridge"
version="latest"
install_root="$HOME/Applications/Local Browser Bridge"
default_install_root="$install_root"
owner_marker=".lbb-install-owner"
owner_marker_value="local-browser-bridge-install-v1"
startup=1
start_helper=0
enable_shell=0
launch=1
uninstall=0
reset_token=0
self_test=0

usage() {
  cat <<'EOF'
Usage: install-macos.sh [--version latest|VERSION] [--install-root PATH]
                        [--no-startup] [--start-helper] [--enable-shell] [--no-launch]
                        [--uninstall] [--reset-token] [--self-test]
EOF
}

while (($#)); do
  case "$1" in
    --version) version="${2:?missing version}"; shift 2 ;;
    --install-root) install_root="${2:?missing install root}"; shift 2 ;;
    --no-startup) startup=0; shift ;;
    --start-helper) start_helper=1; shift ;;
    --enable-shell) enable_shell=1; shift ;;
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
    *$'\n'*|*'/../'*|*/..|*'/./'*|*/.|*'//'*) echo "Install root is not lexically canonical." >&2; exit 1 ;;
  esac
  install_root="${install_root%/}"
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

has_owned_install_layout() {
  local marker="$install_root/$owner_marker"
  if [[ -e "$marker" || -L "$marker" ]]; then
    [[ -f "$marker" && ! -L "$marker" ]] || return 1
    [[ "$(/bin/cat "$marker")" == "$owner_marker_value" ]]
    return
  fi
  [[ "$install_root" == "$default_install_root" &&
     -f "$install_root/local-browser-bridge" &&
     -f "$install_root/extension/manifest.json" &&
     ! -L "$install_root/extension" ]]
}

assert_safe_product_tree() {
  local path=$1
  [[ -e "$path" || -L "$path" ]] || return 0
  [[ -d "$path" && ! -L "$path" ]] || { echo "Refusing a linked or non-directory product path: $path" >&2; exit 1; }
  local linked
  linked="$(/usr/bin/find "$path" -type l -print -quit 2>/dev/null || true)"
  [[ -z "$linked" ]] || { echo "Refusing a product tree containing a symlink: $linked" >&2; exit 1; }
}

assert_safe_product_file() {
  local path=$1
  [[ -e "$path" || -L "$path" ]] || return 0
  [[ -f "$path" && ! -L "$path" ]] || { echo "Refusing a linked or non-file product path: $path" >&2; exit 1; }
}

stop_installed_processes() {
  local pid command
  while read -r pid command; do
    [[ "$pid" =~ ^[0-9]+$ ]] || continue
    case "$command" in
      "$install_root/"*) /bin/kill -TERM "$pid" 2>/dev/null || true ;;
    esac
  done < <(/bin/ps -axo pid=,command=)
  /bin/sleep 0.25
  while read -r pid command; do
    [[ "$pid" =~ ^[0-9]+$ ]] || continue
    case "$command" in
      "$install_root/"*) /bin/kill -KILL "$pid" 2>/dev/null || true ;;
    esac
  done < <(/bin/ps -axo pid=,command=)
}

remove_known_install() {
  [[ -e "$install_root" || -L "$install_root" ]] || return 0
  assert_ordinary_dir "$install_root"
  has_owned_install_layout || { echo "The install directory is not recognized as installer-owned; nothing was removed." >&2; exit 1; }
  local name
  for name in \
    local-browser-bridge SHA256SUMS.txt \
    "Open Local Browser Bridge.command" \
    "Finish Browser Extension Setup.command" \
    "Start Computer Helper.command" \
    "Uninstall Local Browser Bridge.command" \
    "$owner_marker"; do
    assert_safe_product_file "$install_root/$name"
  done
  assert_safe_product_tree "$install_root/Local Computer Helper.app"
  assert_safe_product_tree "$install_root/extension"
  stop_installed_processes
  for name in \
    local-browser-bridge SHA256SUMS.txt \
    "Open Local Browser Bridge.command" \
    "Finish Browser Extension Setup.command" \
    "Start Computer Helper.command" \
    "Uninstall Local Browser Bridge.command"; do
    /bin/rm -f -- "$install_root/$name"
  done
  /bin/rm -rf -- "$install_root/Local Computer Helper.app" "$install_root/extension"
  local unknown=0 entry base
  while IFS= read -r entry; do
    base="${entry##*/}"
    [[ "$base" == "$owner_marker" ]] || unknown=1
  done < <(/usr/bin/find "$install_root" -mindepth 1 -maxdepth 1 -print 2>/dev/null)
  if ((unknown)); then
    echo "Retained the install directory because it contains files not owned by the installer: $install_root" >&2
  else
    /bin/rm -f -- "$install_root/$owner_marker"
    /bin/rmdir -- "$install_root" 2>/dev/null || true
  fi
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

open_extensions_page() {
  if [[ -d '/Applications/Google Chrome.app' ]]; then
    /usr/bin/open -a 'Google Chrome' 'chrome://extensions'
    return 0
  elif [[ -d '/Applications/Microsoft Edge.app' ]]; then
    /usr/bin/open -a 'Microsoft Edge' 'edge://extensions'
    return 0
  fi
  return 1
}

show_extension_setup() {
  local extension_root=$1 token=$2 browser_step
  printf '%s' "$extension_root" | /usr/bin/pbcopy || true
  /usr/bin/open "$extension_root"
  if open_extensions_page; then
    browser_step='The browser extensions page and the extension folder are open.'
  else
    browser_step='Open chrome://extensions or edge://extensions in your browser.'
  fi
  /usr/bin/osascript - "$browser_step" <<'APPLESCRIPT' || true
on run argv
  display dialog "Finish browser setup now:\n\n1. " & item 1 of argv & "\n2. Turn on Developer mode.\n3. Click Load unpacked.\n4. Paste the extension folder path already copied to the clipboard and select it.\n\nComplete steps 1-4, then choose OK. The installer will copy the bridge token next." with title "Finish Local Browser Bridge Setup" buttons {"OK"} default button "OK" with icon note
end run
APPLESCRIPT
  if [[ "$token" =~ ^[A-Za-z0-9_-]{32,}$ ]]; then
    printf '%s' "$token" | /usr/bin/pbcopy
    /usr/bin/osascript -e 'display dialog "The bridge token is now copied. Open Local Browser Bridge, paste it, and choose Save and connect.\n\nYou can repeat this guide by double-clicking Finish Browser Extension Setup.command in the install folder." with title "Connect Local Browser Bridge" buttons {"OK"} default button "OK" with icon note' || true
  else
    /usr/bin/osascript -e 'display dialog "The server token is not ready yet. Double-click Open Local Browser Bridge, then run Finish Browser Extension Setup again." with title "Connect Local Browser Bridge" buttons {"OK"} default button "OK" with icon caution' || true
  fi
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
  if [[ -e "$install_root" || -L "$install_root" ]]; then
    assert_ordinary_dir "$install_root"
    has_owned_install_layout || { echo "The install directory is not recognized as installer-owned; nothing was removed." >&2; exit 1; }
    local_name=''
    for local_name in \
      local-browser-bridge SHA256SUMS.txt \
      "Open Local Browser Bridge.command" \
      "Finish Browser Extension Setup.command" \
      "Start Computer Helper.command" \
      "Uninstall Local Browser Bridge.command" \
      "$owner_marker"; do
      assert_safe_product_file "$install_root/$local_name"
    done
    assert_safe_product_tree "$install_root/Local Computer Helper.app"
    assert_safe_product_tree "$install_root/extension"
  fi
  if [[ -e "$plist" || -L "$plist" ]]; then
    assert_ordinary_dir "$HOME/Library"
    assert_ordinary_dir "$HOME/Library/LaunchAgents"
    [[ -f "$plist" && ! -L "$plist" ]] || { echo "Refusing a linked or non-file LaunchAgent path." >&2; exit 1; }
    /usr/bin/grep -Fq "<string>$label</string>" "$plist" && /usr/bin/grep -Fq 'local-browser-bridge' "$plist" || {
      echo "The LaunchAgent is not recognized as product-owned; nothing was removed from it." >&2; exit 1;
    }
    /bin/launchctl bootout "gui/$(/usr/bin/id -u)/$label" >/dev/null 2>&1 || true
    /bin/rm -f -- "$plist"
  fi
  remove_known_install
  if ((reset_token)); then
    token_directory="$HOME/.local-browser-bridge"
    if [[ -e "$token_directory" || -L "$token_directory" ]]; then assert_ordinary_dir "$token_directory"; fi
    assert_safe_product_file "$token_directory/token"
    /bin/rm -f -- "$token_directory/token"
  fi
  for service in ScreenCapture Accessibility ListenEvent; do
    /usr/bin/tccutil reset "$service" dev.flrngel.local-browser-bridge.computer-helper >/dev/null 2>&1 || true
  done
  open_extensions_page || true
  /usr/bin/osascript -e 'display dialog "The unpacked extension files are gone. If a Local Browser Bridge card remains, click Remove once. Browser profile files were intentionally left untouched." with title "Finish Local Browser Bridge Removal" buttons {"OK"} default button "OK" with icon note' || true
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
if [[ -e "$install_root" || -L "$install_root" ]]; then
  assert_ordinary_dir "$install_root"
  if [[ -n "$(/usr/bin/find "$install_root" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]]; then
    remove_known_install
  fi
fi
/bin/mkdir -p -- "$install_root"
/bin/cp "$stage/local-browser-bridge" "$install_root/local-browser-bridge"
/usr/bin/ditto "$stage/Local Computer Helper.app" "$install_root/Local Computer Helper.app"
/usr/bin/ditto "$stage/extension" "$install_root/extension"
/bin/cp "$stage/SHA256SUMS.txt" "$install_root/SHA256SUMS.txt"
printf '%s\n' "$owner_marker_value" > "$install_root/$owner_marker"

if ((startup)); then
  assert_ordinary_dir "$HOME/Library"
  if [[ -e "$HOME/Library/LaunchAgents" || -L "$HOME/Library/LaunchAgents" ]]; then
    assert_ordinary_dir "$HOME/Library/LaunchAgents"
  else
    /bin/mkdir -- "$HOME/Library/LaunchAgents"
  fi
  escaped_root="$(printf '%s' "$install_root/local-browser-bridge" | /usr/bin/sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')"
  shell_plist_argument=''
  ((enable_shell)) && shell_plist_argument='<string>--enable-shell</string>'
  /bin/cat > "$stage/launchagent.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$label</string>
<key>ProgramArguments</key><array><string>$escaped_root</string>$shell_plist_argument</array>
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

quoted_server="$(printf '%q' "$install_root/local-browser-bridge")"
quoted_helper="$(printf '%q' "$install_root/Local Computer Helper.app")"
quoted_extension="$(printf '%q' "$install_root/extension")"
uninstaller_url="https://raw.githubusercontent.com/$repository/v$resolved/scripts/uninstall-macos.sh"
shell_argument=''
((enable_shell)) && shell_argument=' --enable-shell'
/bin/cat > "$install_root/Open Local Browser Bridge.command" <<EOF
#!/bin/bash
if ! /usr/bin/curl --fail --silent --max-time 1 http://127.0.0.1:17373/health >/dev/null 2>&1; then
  $quoted_server$shell_argument >/dev/null 2>&1 &
fi
for _ in {1..100}; do [[ -f "\$HOME/.local-browser-bridge/token" ]] && break; /bin/sleep 0.1; done
if [[ -f "\$HOME/.local-browser-bridge/token" ]]; then
  token="\$(/bin/cat "\$HOME/.local-browser-bridge/token")"
  /usr/bin/open "http://127.0.0.1:17373/#token=\$token"
else
  /usr/bin/open "http://127.0.0.1:17373/"
fi
EOF
/bin/cat > "$install_root/Finish Browser Extension Setup.command" <<EOF
#!/bin/bash
extension_root=$quoted_extension
printf '%s' "\$extension_root" | /usr/bin/pbcopy || true
/usr/bin/open "\$extension_root"
if [[ -d '/Applications/Google Chrome.app' ]]; then
  /usr/bin/open -a 'Google Chrome' 'chrome://extensions'
elif [[ -d '/Applications/Microsoft Edge.app' ]]; then
  /usr/bin/open -a 'Microsoft Edge' 'edge://extensions'
fi
/usr/bin/osascript -e 'display dialog "Turn on Developer mode, click Load unpacked, and paste the extension folder path already copied to your clipboard." with title "Finish Local Browser Bridge Setup" buttons {"OK"} default button "OK" with icon note'
if [[ -f "\$HOME/.local-browser-bridge/token" ]]; then
  token="\$(/bin/cat "\$HOME/.local-browser-bridge/token")"
  printf '%s' "\$token" | /usr/bin/pbcopy
  /usr/bin/osascript -e 'display dialog "The bridge token is now copied. Open Local Browser Bridge, paste it, and choose Save and connect." with title "Connect Local Browser Bridge" buttons {"OK"} default button "OK" with icon note'
fi
EOF
/bin/cat > "$install_root/Start Computer Helper.command" <<EOF
#!/bin/bash
/usr/bin/open $quoted_helper
EOF
/bin/cat > "$install_root/Uninstall Local Browser Bridge.command" <<EOF
#!/bin/bash
script="\$(/usr/bin/mktemp \"\${TMPDIR:-/tmp}/lbb-uninstall.XXXXXX\")" || exit 1
trap '/bin/rm -f -- "\$script"' EXIT
/usr/bin/curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 '$uninstaller_url' --output "\$script" || exit 1
/bin/bash "\$script"
EOF
/bin/chmod 700 "$install_root/Open Local Browser Bridge.command" "$install_root/Finish Browser Extension Setup.command" "$install_root/Start Computer Helper.command" "$install_root/Uninstall Local Browser Bridge.command"

if ((launch)); then
  if ((!startup)); then
    if ((enable_shell)); then
      "$install_root/local-browser-bridge" --enable-shell >/dev/null 2>&1 &
    else
      "$install_root/local-browser-bridge" >/dev/null 2>&1 &
    fi
  fi
  if ((start_helper)); then /usr/bin/open "$install_root/Local Computer Helper.app"; fi
  token_path="$HOME/.local-browser-bridge/token"
  for _ in {1..100}; do [[ -f "$token_path" ]] && break; /bin/sleep 0.1; done
  token=''
  if [[ -f "$token_path" ]]; then
    token="$(/bin/cat "$token_path")"
    [[ "$token" =~ ^[A-Za-z0-9_-]{32,}$ ]] && /usr/bin/open "http://127.0.0.1:17373/#token=$token"
  fi
  show_extension_setup "$install_root/extension" "$token"
fi

echo "Installed Local Browser Bridge $resolved for the current user."
echo "Extension folder: $install_root/extension"
echo "Finish setup: double-click $install_root/Finish Browser Extension Setup.command"
echo "Open later: double-click $install_root/Open Local Browser Bridge.command"
if ((enable_shell)); then
  echo "WARNING: Full current-user shell access is enabled for authenticated local API clients." >&2
else
  echo "Shell access is off. Re-run this installer with --enable-shell only if you intend to grant it."
fi
((start_helper)) || echo "Desktop control remains off. Open Local Computer Helper.app only when you want it."
