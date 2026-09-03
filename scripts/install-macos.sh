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
no_desktop_control=0
no_shell=0
launch=1
uninstall=0
reset_token=0
self_test=0

usage() {
  cat <<'EOF'
Usage: install-macos.sh [--version latest|VERSION] [--install-root PATH]
                        [--no-startup] [--no-desktop-control] [--no-shell] [--no-launch]
                        [--start-helper] [--enable-shell]
                        [--uninstall] [--reset-token] [--self-test]

Desktop control (the computer helper) and shell access are on by default.
Pass --no-desktop-control / --no-shell to opt out. --start-helper and
--enable-shell are still accepted as no-op aliases for existing commands.
EOF
}

while (($#)); do
  case "$1" in
    --version) version="${2:?missing version}"; shift 2 ;;
    --install-root) install_root="${2:?missing install root}"; shift 2 ;;
    --no-startup) startup=0; shift ;;
    --start-helper) start_helper=1; shift ;;
    --enable-shell) enable_shell=1; shift ;;
    --no-desktop-control) no_desktop_control=1; shift ;;
    --no-shell) no_shell=1; shift ;;
    --no-launch) launch=0; shift ;;
    --uninstall) uninstall=1; shift ;;
    --reset-token) reset_token=1; shift ;;
    --self-test) self_test=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

# Desktop control and shell access are on by default; --no-desktop-control / --no-shell
# opt out. --start-helper and --enable-shell remain accepted no-op aliases.
((no_desktop_control)) && start_helper=0 || start_helper=1
((no_shell)) && enable_shell=0 || enable_shell=1

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
  assert_safe_product_tree "$install_root/Local Browser Bridge.app"
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
  /bin/rm -rf -- "$install_root/Local Browser Bridge.app" "$install_root/Local Computer Helper.app" "$install_root/extension"
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
JXA
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
  echo ""
  echo "== Finish Local Browser Bridge Setup =="
  echo "1. $browser_step"
  echo "2. Turn on Developer mode."
  echo "3. Click Load unpacked."
  echo "4. Paste the extension folder path already copied to the clipboard and select it."
  echo "Complete steps 1-4. The bridge token is copied to your clipboard next."
  if [[ "$token" =~ ^[A-Za-z0-9_-]{32,}$ ]]; then
    printf '%s' "$token" | /usr/bin/pbcopy
    echo "The bridge token is now copied. Open Local Browser Bridge, paste it, and choose Save and connect."
    echo "You can repeat this guide by double-clicking Finish Browser Extension Setup.command in the install folder."
  else
    echo "The server token is not ready yet. Double-click Open Local Browser Bridge, then run Finish Browser Extension Setup again."
  fi
}

# Reads a flat, top-level `"key": true|false` field out of JSON text $1.
# Prints "true" or "false" and returns success only when that exact shape is
# found; otherwise prints nothing and fails, so callers can tell "absent or
# not a plain boolean" apart from a real value.
json_flat_bool() {
  local json="$1" key="$2"
  local pattern
  pattern='"'"$key"'"[[:space:]]*:[[:space:]]*(true|false)'
  [[ "$json" =~ $pattern ]] || return 1
  printf '%s' "${BASH_REMATCH[1]}"
}

write_settings() {
  local settings_dir="${1:-$HOME/.local-browser-bridge}"
  local settings_path="$settings_dir/settings.json"
  /bin/mkdir -p -- "$settings_dir"

  local shell_json desktop_json startup_json
  shell_json=false; ((enable_shell)) && shell_json=true
  desktop_json=false; ((start_helper)) && desktop_json=true
  startup_json=false; ((startup)) && startup_json=true

  # Merge, do not regenerate: only the field whose opt-out flag was actually
  # passed this run may change. Every other field - startAtLogin included,
  # plus any field a newer version of this script does not know about - is
  # left byte-for-byte as it was in the existing file, so a re-install or
  # upgrade never silently re-enables a capability the user had previously
  # turned off. A missing, unreadable, or corrupt existing file (one where
  # the three known fields cannot all be read back as plain booleans) falls
  # back to this run's freshly computed defaults instead of failing the
  # install.
  local existing="" have_existing=0
  if [[ -f "$settings_path" ]]; then
    existing="$(/bin/cat -- "$settings_path" 2>/dev/null)" || existing=""
    if [[ -n "$existing" ]] \
      && json_flat_bool "$existing" shellEnabled >/dev/null \
      && json_flat_bool "$existing" desktopControlEnabled >/dev/null \
      && json_flat_bool "$existing" startAtLogin >/dev/null; then
      have_existing=1
    fi
  fi

  local body tmp
  if ((have_existing)); then
    body="$existing"
    if ((no_shell)); then
      body="$(printf '%s' "$body" | /usr/bin/sed -E 's/("shellEnabled"[[:space:]]*:[[:space:]]*)(true|false)/\1'"$shell_json"'/')"
    fi
    if ((no_desktop_control)); then
      body="$(printf '%s' "$body" | /usr/bin/sed -E 's/("desktopControlEnabled"[[:space:]]*:[[:space:]]*)(true|false)/\1'"$desktop_json"'/')"
    fi
    if ((! startup)); then
      body="$(printf '%s' "$body" | /usr/bin/sed -E 's/("startAtLogin"[[:space:]]*:[[:space:]]*)(true|false)/\1'"$startup_json"'/')"
    fi
  else
    body="$(printf '{"version":1,"shellEnabled":%s,"desktopControlEnabled":%s,"startAtLogin":%s}' \
      "$shell_json" "$desktop_json" "$startup_json")"
  fi

  tmp="$(/usr/bin/mktemp "$settings_dir/settings.json.XXXXXX")"
  printf '%s\n' "$body" > "$tmp"
  /bin/chmod 600 "$tmp"
  /bin/mv -f -- "$tmp" "$settings_path"
}

label="dev.flrngel.local-browser-bridge"
plist="$HOME/Library/LaunchAgents/$label.plist"
assert_safe_install_root

settings_merge_self_test() {
  local scratch="$1"
  local settings_dir="$scratch/settings-test"
  local settings_path="$settings_dir/settings.json"
  local saved_no_shell=$no_shell saved_no_desktop_control=$no_desktop_control saved_startup=$startup
  local saved_enable_shell=$enable_shell saved_start_helper=$start_helper

  # Fresh install (no existing file): full defaults are written.
  no_shell=0; no_desktop_control=0; startup=1
  enable_shell=1; start_helper=1
  write_settings "$settings_dir"
  local written
  written="$(/bin/cat -- "$settings_path")"
  [[ "$(json_flat_bool "$written" shellEnabled)" == true ]] || { echo "Settings self-test failed: fresh install did not enable shellEnabled." >&2; exit 1; }
  [[ "$(json_flat_bool "$written" desktopControlEnabled)" == true ]] || { echo "Settings self-test failed: fresh install did not enable desktopControlEnabled." >&2; exit 1; }
  [[ "$(json_flat_bool "$written" startAtLogin)" == true ]] || { echo "Settings self-test failed: fresh install did not enable startAtLogin." >&2; exit 1; }

  # Re-install passing only --no-shell must turn shellEnabled off without
  # touching desktopControlEnabled or startAtLogin.
  no_shell=1; no_desktop_control=0; startup=1
  enable_shell=0; start_helper=1
  write_settings "$settings_dir"
  local merged
  merged="$(/bin/cat -- "$settings_path")"
  [[ "$(json_flat_bool "$merged" shellEnabled)" == false ]] || { echo "Settings self-test failed: --no-shell did not disable shellEnabled." >&2; exit 1; }
  [[ "$(json_flat_bool "$merged" desktopControlEnabled)" == true ]] || { echo "Settings self-test failed: unrelated --no-shell run clobbered desktopControlEnabled." >&2; exit 1; }
  [[ "$(json_flat_bool "$merged" startAtLogin)" == true ]] || { echo "Settings self-test failed: unrelated --no-shell run clobbered startAtLogin." >&2; exit 1; }

  # An unknown field from a newer version of this script must survive a
  # merge unchanged.
  printf '{"version":1,"shellEnabled":false,"desktopControlEnabled":true,"startAtLogin":true,"futureField":"keep-me"}' > "$settings_path"
  no_shell=0; no_desktop_control=1; startup=1
  enable_shell=1; start_helper=0
  write_settings "$settings_dir"
  local merged_extra
  merged_extra="$(/bin/cat -- "$settings_path")"
  [[ "$(json_flat_bool "$merged_extra" desktopControlEnabled)" == false ]] || { echo "Settings self-test failed: --no-desktop-control did not disable desktopControlEnabled." >&2; exit 1; }
  [[ "$(json_flat_bool "$merged_extra" shellEnabled)" == false ]] || { echo "Settings self-test failed: unrelated --no-desktop-control run clobbered shellEnabled." >&2; exit 1; }
  [[ "$merged_extra" == *'"futureField":"keep-me"'* ]] || { echo "Settings self-test failed: unknown field was not preserved through a merge." >&2; exit 1; }

  # A corrupt existing file must fall back to this run's computed defaults
  # instead of failing the install.
  printf '{not valid json' > "$settings_path"
  no_shell=1; no_desktop_control=0; startup=1
  enable_shell=0; start_helper=1
  write_settings "$settings_dir"
  local recovered
  recovered="$(/bin/cat -- "$settings_path")"
  [[ "$(json_flat_bool "$recovered" shellEnabled)" == false ]] || { echo "Settings self-test failed: corrupt file was not recovered with this run's defaults (shellEnabled)." >&2; exit 1; }
  [[ "$(json_flat_bool "$recovered" desktopControlEnabled)" == true ]] || { echo "Settings self-test failed: corrupt file was not recovered with this run's defaults (desktopControlEnabled)." >&2; exit 1; }
  [[ "$(json_flat_bool "$recovered" startAtLogin)" == true ]] || { echo "Settings self-test failed: corrupt file was not recovered with this run's defaults (startAtLogin)." >&2; exit 1; }

  no_shell=$saved_no_shell; no_desktop_control=$saved_no_desktop_control; startup=$saved_startup
  enable_shell=$saved_enable_shell; start_helper=$saved_start_helper
}

if ((self_test)); then
  scratch="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/lbb-installer-self-test.XXXXXX")"
  trap '/bin/rm -rf -- "$scratch"' EXIT
  printf '%064d  file1.bin\n%064d  file2.bin\n%064d  file3.bin\n%064d  file4.bin\n' 0 0 0 0 > "$scratch/SHA256SUMS.txt"
  [[ "$(manifest_value "$scratch/SHA256SUMS.txt" file4.bin)" == "$(printf '%064d' 0)" ]]
  settings_merge_self_test "$scratch"
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
    assert_safe_product_tree "$install_root/Local Browser Bridge.app"
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
  echo "Removed installed program files: $install_root"
  if ((reset_token)); then
    token_directory="$HOME/.local-browser-bridge"
    if [[ -e "$token_directory" || -L "$token_directory" ]]; then assert_ordinary_dir "$token_directory"; fi
    assert_safe_product_file "$token_directory/token"
    /bin/rm -f -- "$token_directory/token"
    echo "Removed the bridge token."
  fi
  settings_directory="$HOME/.local-browser-bridge"
  if [[ -e "$settings_directory" || -L "$settings_directory" ]]; then assert_ordinary_dir "$settings_directory"; fi
  assert_safe_product_file "$settings_directory/settings.json"
  if [[ -f "$settings_directory/settings.json" ]]; then
    /bin/rm -f -- "$settings_directory/settings.json"
    echo "Removed settings.json."
  fi
  for service in ScreenCapture Accessibility ListenEvent; do
    /usr/bin/tccutil reset "$service" dev.flrngel.local-browser-bridge.computer-helper >/dev/null 2>&1 || true
  done
  open_extensions_page || true
  echo "The unpacked extension files are gone. If a Local Browser Bridge card remains in the extensions page, click Remove once."
  echo "Browser profile files were intentionally left untouched."
  echo "Local Browser Bridge was removed for the current user."
  exit 0
fi

[[ "$(/usr/bin/sw_vers -productVersion | /usr/bin/awk -F. '{print $1}')" -ge 13 ]] || { echo "macOS 13 or later is required." >&2; exit 1; }
if [[ "$version" != latest && ! "$version" =~ ^v?[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Version must be 'latest' or a stable semantic version." >&2; exit 2
fi

stage="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/lbb-install.XXXXXX")"
trap '/bin/rm -rf -- "$stage"' EXIT
echo "Resolving release $version..."
api="https://api.github.com/repos/$repository/releases/latest"
[[ "$version" == latest ]] || api="https://api.github.com/repos/$repository/releases/tags/v${version#v}"
/usr/bin/curl --fail --silent --show-error --location --header 'Accept: application/vnd.github+json' --user-agent local-browser-bridge-installer "$api" --output "$stage/release.json"
parse_release "$stage/release.json" "$stage/release.tsv"
resolved="$(/usr/bin/awk -F '\t' '$1 == "VERSION" { print $2 }' "$stage/release.tsv")"
[[ "$resolved" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "Release version parsing failed." >&2; exit 1; }
echo "Resolved release v$resolved."
archive="local-browser-bridge-v$resolved-macos-universal.tar.gz"
extension_zip="local-browser-bridge-extension-v$resolved.zip"

for name in "$archive" "$extension_zip" SHA256SUMS.txt; do
  echo "Downloading $name..."
  expected="$(/usr/bin/awk -F '\t' -v n="$name" '$1 == n { print $2 }' "$stage/release.tsv")"
  url="$(/usr/bin/awk -F '\t' -v n="$name" '$1 == n { print $3 }' "$stage/release.tsv")"
  [[ "$expected" =~ ^[0-9a-f]{64}$ && "$url" == https://github.com/* ]] || { echo "Missing trusted metadata for $name" >&2; exit 1; }
  /usr/bin/curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 "$url" --output "$stage/$name"
  echo "Checking $name..."
  [[ "$(sha256 "$stage/$name")" == "$expected" ]] || { echo "GitHub digest mismatch for $name" >&2; exit 1; }
done

echo "Checking the release manifest..."
[[ "$(/usr/bin/wc -l < "$stage/SHA256SUMS.txt" | /usr/bin/tr -d ' ')" == 4 ]] || { echo "The checksum manifest is not canonical." >&2; exit 1; }
for name in "$archive" "$extension_zip"; do
  [[ "$(manifest_value "$stage/SHA256SUMS.txt" "$name")" == "$(sha256 "$stage/$name")" ]] || { echo "Manifest digest mismatch for $name" >&2; exit 1; }
done

echo "Extracting Local Browser Bridge..."
/usr/bin/tar -xzf "$stage/$archive" -C "$stage"
/usr/bin/ditto -x -k "$stage/$extension_zip" "$stage/extension"
[[ -x "$stage/local-browser-bridge" && -d "$stage/Local Browser Bridge.app" && -d "$stage/Local Computer Helper.app" && -f "$stage/extension/manifest.json" ]] || { echo "A package has an unexpected layout." >&2; exit 1; }

echo "Installing Local Browser Bridge to $install_root..."
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
/usr/bin/ditto "$stage/Local Browser Bridge.app" "$install_root/Local Browser Bridge.app"
/usr/bin/ditto "$stage/Local Computer Helper.app" "$install_root/Local Computer Helper.app"
/usr/bin/ditto "$stage/extension" "$install_root/extension"
/bin/cp "$stage/SHA256SUMS.txt" "$install_root/SHA256SUMS.txt"
printf '%s\n' "$owner_marker_value" > "$install_root/$owner_marker"

echo "Saving settings..."
write_settings

if ((startup)); then
  assert_ordinary_dir "$HOME/Library"
  if [[ -e "$HOME/Library/LaunchAgents" || -L "$HOME/Library/LaunchAgents" ]]; then
    assert_ordinary_dir "$HOME/Library/LaunchAgents"
  else
    /bin/mkdir -- "$HOME/Library/LaunchAgents"
  fi
  escaped_root="$(printf '%s' "$install_root/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop" | /usr/bin/sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')"
  shell_plist_argument=''
  ((enable_shell)) && shell_plist_argument='<string>--enable-shell</string>'
  helper_plist_argument=''
  ((start_helper)) && helper_plist_argument='<string>--start-helper</string>'
  /bin/cat > "$stage/launchagent.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$label</string>
<key>ProgramArguments</key><array><string>$escaped_root</string>$shell_plist_argument$helper_plist_argument</array>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
</dict></plist>
EOF
  /usr/bin/plutil -lint "$stage/launchagent.plist" >/dev/null
  /bin/launchctl bootout "gui/$(/usr/bin/id -u)/$label" >/dev/null 2>&1 || true
  /bin/cp "$stage/launchagent.plist" "$plist"
  if ((launch)); then
    /bin/launchctl bootstrap "gui/$(/usr/bin/id -u)" "$plist"
  fi
else
  /bin/launchctl bootout "gui/$(/usr/bin/id -u)/$label" >/dev/null 2>&1 || true
  /bin/rm -f -- "$plist"
fi

quoted_desktop="$(printf '%q' "$install_root/Local Browser Bridge.app")"
quoted_helper="$(printf '%q' "$install_root/Local Computer Helper.app")"
quoted_extension="$(printf '%q' "$install_root/extension")"
uninstaller_url="https://raw.githubusercontent.com/$repository/v$resolved/scripts/uninstall-macos.sh"
shell_argument=''
((enable_shell)) && shell_argument=' --enable-shell'
/bin/cat > "$install_root/Open Local Browser Bridge.command" <<EOF
#!/bin/bash
/usr/bin/open $quoted_desktop --args$shell_argument
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
  echo "Starting Local Browser Bridge..."
  if ((!startup)); then
    desktop_launch_arguments=()
    ((enable_shell)) && desktop_launch_arguments+=(--enable-shell)
    ((start_helper)) && desktop_launch_arguments+=(--start-helper)
    /usr/bin/open "$install_root/Local Browser Bridge.app" --args "${desktop_launch_arguments[@]}"
  fi
  if ((startup && start_helper)); then /usr/bin/open "$install_root/Local Computer Helper.app"; fi
  token_path="$HOME/.local-browser-bridge/token"
  echo "Waiting for the authentication token..."
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
echo "Open later: open $install_root/Local Browser Bridge.app"
if ((enable_shell)); then
  echo "Shell access is on by default: authenticated local API clients can run shell commands as you. Add --no-shell to turn it off."
else
  echo "Shell access is off. Re-run this installer with --enable-shell only if you intend to grant it."
fi
if ((start_helper)); then
  echo "Desktop control is on by default: this computer can be observed and controlled through the bridge. Add --no-desktop-control to turn it off."
else
  echo "Desktop control is off. Re-run this installer with --start-helper only if you intend to grant it."
fi
