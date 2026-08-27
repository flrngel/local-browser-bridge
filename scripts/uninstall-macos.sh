#!/usr/bin/env bash
set -euo pipefail

product_name="Local Browser Bridge"
install_root="$HOME/Applications/Local Browser Bridge"
default_install_root="$install_root"
owner_marker=".lbb-install-owner"
owner_marker_value="local-browser-bridge-install-v1"
launch_label="dev.flrngel.local-browser-bridge"
launch_agent="$HOME/Library/LaunchAgents/$launch_label.plist"
token_path="$HOME/.local-browser-bridge/token"
keep_token=0
keep_permissions=0
no_browser=0
dry_run=0
self_test=0
removed_install=0
self_test_scratch=''

usage() {
  cat <<'EOF'
Usage: uninstall-macos.sh [--install-root PATH] [--keep-token]
                          [--keep-permissions] [--no-browser]
                          [--dry-run] [--self-test]

Removes only Local Browser Bridge files owned by the current-user installer.
Browser profile files are never edited. Chrome or Edge may require one final
click on Remove for the now-missing unpacked extension.
EOF
}

while (($#)); do
  case "$1" in
    --install-root) install_root="${2:?missing install root}"; shift 2 ;;
    --keep-token) keep_token=1; shift ;;
    --keep-permissions) keep_permissions=1; shift ;;
    --no-browser) no_browser=1; shift ;;
    --dry-run) dry_run=1; shift ;;
    --self-test) self_test=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown option: $1" >&2; usage >&2; exit 2 ;;
  esac
done

say_action() {
  if ((dry_run)); then
    printf 'Would %s\n' "$*"
  else
    printf '%s\n' "$*"
  fi
}

fail() {
  echo "$*" >&2
  exit 1
}

is_symlink() { [[ -L "$1" ]]; }

assert_ordinary_directory() {
  local path=$1
  [[ -d "$path" ]] || fail "Required directory does not exist: $path"
  is_symlink "$path" && fail "Refusing a symlink directory: $path"
  return 0
}

assert_safe_install_root() {
  [[ -n "$install_root" && "$install_root" == /* ]] || fail "Install root must be an absolute path."
  case "$install_root" in
    *$'\n'*|*'/../'*|*/..|*'/./'*|*/.|*'//'*) fail "Install root is not lexically canonical." ;;
  esac
  install_root="${install_root%/}"
  case "$install_root" in
    "$HOME"/*) ;;
    *) fail "Install root must be a child of the current user's home directory." ;;
  esac
  [[ "$install_root" != "$HOME" && "$install_root" != / ]] || fail "Refusing a broad install root."

  local cursor="$install_root"
  while [[ "$cursor" != "$HOME" ]]; do
    if [[ -e "$cursor" || -L "$cursor" ]]; then
      assert_ordinary_directory "$cursor"
    fi
    cursor="$(/usr/bin/dirname "$cursor")"
  done
  assert_ordinary_directory "$HOME"
}

has_valid_owner_marker() {
  local marker="$install_root/$owner_marker"
  [[ -f "$marker" && ! -L "$marker" ]] || return 1
  [[ "$(/bin/cat "$marker")" == "$owner_marker_value" ]]
}

has_legacy_default_layout() {
  [[ "$install_root" == "$default_install_root" ]] || return 1
  [[ -f "$install_root/local-browser-bridge" && ! -L "$install_root/local-browser-bridge" ]] || return 1
  [[ -f "$install_root/extension/manifest.json" && ! -L "$install_root/extension" ]] || return 1
}

assert_owned_install_root() {
  [[ -e "$install_root" || -L "$install_root" ]] || return 0
  assert_ordinary_directory "$install_root"
  if [[ -e "$install_root/$owner_marker" || -L "$install_root/$owner_marker" ]]; then
    has_valid_owner_marker || fail "The install ownership marker is invalid; nothing was removed."
    return 0
  fi
  has_legacy_default_layout || fail "The directory is not a recognized installer-owned Local Browser Bridge installation: $install_root"
}

assert_ordinary_file_or_missing() {
  local path=$1
  [[ -e "$path" || -L "$path" ]] || return 0
  [[ -f "$path" && ! -L "$path" ]] || fail "Refusing a linked or non-file product path: $path"
}

assert_ordinary_tree_or_missing() {
  local path=$1
  [[ -e "$path" || -L "$path" ]] || return 0
  [[ -d "$path" && ! -L "$path" ]] || fail "Refusing a linked or non-directory product path: $path"
  local linked
  linked="$(/usr/bin/find "$path" -type l -print -quit 2>/dev/null || true)"
  [[ -z "$linked" ]] || fail "Refusing a product tree containing a symlink: $linked"
}

preflight_install_entries() {
  local name
  for name in \
    "local-browser-bridge" \
    "SHA256SUMS.txt" \
    "Open Local Browser Bridge.command" \
    "Finish Browser Extension Setup.command" \
    "Start Computer Helper.command" \
    "Uninstall Local Browser Bridge.command" \
    "$owner_marker"; do
    assert_ordinary_file_or_missing "$install_root/$name"
  done
  assert_ordinary_tree_or_missing "$install_root/Local Computer Helper.app"
  assert_ordinary_tree_or_missing "$install_root/Local Browser Bridge.app"
  assert_ordinary_tree_or_missing "$install_root/extension"
}

is_allowlisted_top_level_name() {
  case "$1" in
    local-browser-bridge|SHA256SUMS.txt|\
    'Local Browser Bridge.app'|'Local Computer Helper.app'|extension|\
    'Open Local Browser Bridge.command'|\
    'Finish Browser Extension Setup.command'|\
    'Start Computer Helper.command'|\
    'Uninstall Local Browser Bridge.command'|\
    "$owner_marker") return 0 ;;
    *) return 1 ;;
  esac
}

install_root_has_unknown_entries() {
  local entry name
  while IFS= read -r entry; do
    name="${entry##*/}"
    is_allowlisted_top_level_name "$name" || return 0
  done < <(/usr/bin/find "$install_root" -mindepth 1 -maxdepth 1 -print 2>/dev/null)
  return 1
}

remove_file() {
  local path=$1
  [[ -e "$path" ]] || return 0
  if ((dry_run)); then say_action "remove file: $path"; else /bin/rm -f -- "$path"; fi
}

remove_tree() {
  local path=$1
  [[ -e "$path" ]] || return 0
  if ((dry_run)); then say_action "remove product directory: $path"; else /bin/rm -rf -- "$path"; fi
}

command_for_pid() {
  /bin/ps -p "$1" -o command= 2>/dev/null || true
}

stop_installed_processes() {
  local pid command
  while read -r pid command; do
    [[ "$pid" =~ ^[0-9]+$ ]] || continue
    case "$command" in
      "$install_root/"*)
        say_action "stop product process $pid"
        ((dry_run)) || /bin/kill -TERM "$pid" 2>/dev/null || true
        ;;
    esac
  done < <(/bin/ps -axo pid=,command=)

  ((dry_run)) && return 0
  /bin/sleep 0.25
  while read -r pid command; do
    [[ "$pid" =~ ^[0-9]+$ ]] || continue
    case "$command" in
      "$install_root/"*)
        [[ "$(command_for_pid "$pid")" == "$install_root/"* ]] && /bin/kill -KILL "$pid" 2>/dev/null || true
        ;;
    esac
  done < <(/bin/ps -axo pid=,command=)
}

is_owned_launch_agent() {
  [[ -f "$launch_agent" && ! -L "$launch_agent" ]] || return 1
  /usr/bin/grep -Fq "<string>$launch_label</string>" "$launch_agent" &&
    /usr/bin/grep -Fq "local-browser-bridge" "$launch_agent"
}

remove_launch_agent() {
  [[ -e "$launch_agent" || -L "$launch_agent" ]] || return 0
  assert_ordinary_directory "$HOME/Library"
  assert_ordinary_directory "$HOME/Library/LaunchAgents"
  [[ -f "$launch_agent" && ! -L "$launch_agent" ]] || fail "Refusing a linked or non-file LaunchAgent path: $launch_agent"
  is_owned_launch_agent || fail "The LaunchAgent does not match Local Browser Bridge; nothing was removed from it."
  say_action "unload LaunchAgent $launch_label"
  if ((!dry_run)) && [[ "$(/usr/bin/uname -s)" == Darwin ]]; then
    /bin/launchctl bootout "gui/$(/usr/bin/id -u)/$launch_label" >/dev/null 2>&1 || true
  fi
  remove_file "$launch_agent"
}

remove_install_root() {
  [[ -e "$install_root" || -L "$install_root" ]] || return 0
  assert_owned_install_root
  preflight_install_entries
  removed_install=1
  stop_installed_processes

  local name
  for name in \
    "local-browser-bridge" \
    "SHA256SUMS.txt" \
    "Open Local Browser Bridge.command" \
    "Finish Browser Extension Setup.command" \
    "Start Computer Helper.command" \
    "Uninstall Local Browser Bridge.command"; do
    remove_file "$install_root/$name"
  done
  remove_tree "$install_root/Local Computer Helper.app"
  remove_tree "$install_root/Local Browser Bridge.app"
  remove_tree "$install_root/extension"

  if install_root_has_unknown_entries; then
    echo "Retained the install directory because it contains files not owned by the installer: $install_root" >&2
  else
    remove_file "$install_root/$owner_marker"
    if ((dry_run)); then
      say_action "remove empty install directory: $install_root"
    else
      /bin/rmdir -- "$install_root" 2>/dev/null || true
    fi
  fi
}

remove_token() {
  ((keep_token)) && { echo "Kept the bridge token by request."; return 0; }
  local token_dir
  token_dir="$(/usr/bin/dirname "$token_path")"
  if [[ -e "$token_dir" || -L "$token_dir" ]]; then
    assert_ordinary_directory "$token_dir"
  fi
  if [[ -e "$token_path" || -L "$token_path" ]]; then
    [[ -f "$token_path" && ! -L "$token_path" ]] || fail "Refusing a linked or non-file token path: $token_path"
    remove_file "$token_path"
  fi
  if [[ -d "$token_dir" && ! -L "$token_dir" ]]; then
    if ((dry_run)); then
      [[ -z "$(/usr/bin/find "$token_dir" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]] && say_action "remove empty token directory: $token_dir"
    else
      /bin/rmdir -- "$token_dir" 2>/dev/null || true
    fi
  fi
}

reset_privacy_permissions() {
  ((keep_permissions || !removed_install || dry_run)) && return 0
  [[ "$(/usr/bin/uname -s)" == Darwin ]] || return 0
  local service
  for service in ScreenCapture Accessibility ListenEvent; do
    /usr/bin/tccutil reset "$service" dev.flrngel.local-browser-bridge.computer-helper >/dev/null 2>&1 || true
  done
  echo "Requested removal of the helper's macOS privacy grants."
}

finish_browser_cleanup() {
  ((no_browser || !removed_install || dry_run)) && return 0
  [[ "$(/usr/bin/uname -s)" == Darwin ]] || return 0
  local opened=0
  if [[ -d '/Applications/Google Chrome.app' ]]; then
    /usr/bin/open -a 'Google Chrome' 'chrome://extensions' || true
    opened=1
  fi
  if [[ -d '/Applications/Microsoft Edge.app' ]]; then
    /usr/bin/open -a 'Microsoft Edge' 'edge://extensions' || true
    opened=1
  fi
  local browser_step='Open chrome://extensions or edge://extensions.'
  ((opened)) && browser_step='The installed browser extensions page is open.'
  /usr/bin/osascript - "$browser_step" <<'APPLESCRIPT' || true
on run argv
  display dialog (item 1 of argv) & "\n\nThe unpacked extension files are gone. If a Local Browser Bridge card remains, click Remove once. Browser profile files were intentionally left untouched." with title "Finish Local Browser Bridge Removal" buttons {"OK"} default button "OK" with icon note
end run
APPLESCRIPT
}

invoke_self_test() {
  local scratch original_root original_default original_dry
  local scratch_parent="${TMPDIR:-/tmp}"
  scratch_parent="${scratch_parent%/}"
  scratch="$(/usr/bin/mktemp -d "$scratch_parent/lbb-uninstaller-self-test.XXXXXX")"
  self_test_scratch=$scratch
  trap '[[ -z "${self_test_scratch:-}" ]] || /bin/rm -rf -- "$self_test_scratch"' EXIT
  original_root=$install_root
  original_default=$default_install_root
  original_dry=$dry_run
  install_root="$scratch/home/Applications/Local Browser Bridge"
  default_install_root="$install_root"
  /bin/mkdir -p "$install_root/extension" "$install_root/Local Computer Helper.app"
  printf '%s\n' "$owner_marker_value" > "$install_root/$owner_marker"
  printf '{}\n' > "$install_root/extension/manifest.json"
  printf 'binary\n' > "$install_root/local-browser-bridge"
  printf 'keep\n' > "$install_root/user-note.txt"

  HOME="$scratch/home"
  assert_safe_install_root
  assert_owned_install_root
  preflight_install_entries
  dry_run=1
  remove_install_root
  [[ -f "$install_root/local-browser-bridge" ]] || fail "Dry-run self-test changed product files."
  dry_run=0
  remove_install_root
  [[ ! -e "$install_root/local-browser-bridge" && ! -e "$install_root/extension" ]] || fail "Allowlist removal self-test failed."
  [[ -f "$install_root/user-note.txt" && -f "$install_root/$owner_marker" ]] || fail "Unknown-file retention self-test failed."
  remove_install_root

  local linked_root="$HOME/Applications/Linked LBB"
  install_root=$linked_root
  /bin/mkdir -p "$install_root/extension"
  printf '%s\n' "$owner_marker_value" > "$install_root/$owner_marker"
  /bin/ln -s "$scratch/outside" "$install_root/extension/escape"
  if (preflight_install_entries >/dev/null 2>&1); then
    fail "Symlink refusal self-test failed."
  fi

  local unowned_root="$HOME/Applications/Unowned LBB"
  install_root=$unowned_root
  /bin/mkdir -p "$install_root/extension"
  printf 'binary\n' > "$install_root/local-browser-bridge"
  printf '{}\n' > "$install_root/extension/manifest.json"
  if (assert_owned_install_root >/dev/null 2>&1); then
    fail "Custom-root ownership refusal self-test failed."
  fi

  if (install_root="$HOME"; assert_safe_install_root >/dev/null 2>&1); then
    fail "Broad-root refusal self-test failed."
  fi

  install_root=$original_root
  default_install_root=$original_default
  dry_run=$original_dry
  /bin/rm -rf -- "$self_test_scratch"
  self_test_scratch=''
  echo "macOS one-command uninstaller self-test passed."
}

if ((self_test)); then
  invoke_self_test
  exit 0
fi

[[ "$(/usr/bin/uname -s)" == Darwin ]] || fail "This uninstaller supports macOS only."
assert_safe_install_root
assert_owned_install_root
if [[ -e "$install_root" || -L "$install_root" ]]; then preflight_install_entries; fi
remove_launch_agent
remove_install_root
remove_token
reset_privacy_permissions
finish_browser_cleanup

if ((dry_run)); then
  echo "Dry run complete. No files, processes, permissions, or browser state were changed."
else
  echo "$product_name was removed for the current user."
  echo "Browser profiles were not edited. Remove any stale extension card from the extensions page."
fi
