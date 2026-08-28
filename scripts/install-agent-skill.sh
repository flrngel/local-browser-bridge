#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
project_root="$(cd "$script_dir/.." && pwd -P)"
source_dir="$project_root/skills/local-browser-bridge"
target="agents"
destination=""
check_only=0

usage() {
  cat <<'EOF'
Usage: install-agent-skill.sh [--target agents|codex|claude] [--destination DIR] [--check]

Installs the Local Browser Bridge Agent Skill from this source checkout.
The default target is the cross-client ~/.agents/skills directory.

  --target agents   Install under ~/.agents/skills (default)
  --target codex    Install under $CODEX_HOME/skills or ~/.codex/skills
  --target claude   Install under $CLAUDE_HOME/skills or ~/.claude/skills
  --destination DIR Install under an explicit client skills directory
  --check           Verify an existing installation without changing it
  --self-test       Exercise fresh install, exact check, and drift refusal
EOF
}

resolve_destination() {
  if [[ -n "$destination" ]]; then
    printf '%s\n' "$destination"
    return
  fi
  case "$target" in
    agents) printf '%s\n' "${AGENTS_HOME:-$HOME/.agents}/skills" ;;
    codex) printf '%s\n' "${CODEX_HOME:-$HOME/.codex}/skills" ;;
    claude) printf '%s\n' "${CLAUDE_HOME:-$HOME/.claude}/skills" ;;
    *) echo "Unsupported skill target: $target" >&2; exit 2 ;;
  esac
}

install_or_check() {
  local skills_dir installed_dir stage_parent staged_skill actual_inventory expected_inventory
  skills_dir="$(resolve_destination)"
  [[ -n "$skills_dir" && "$skills_dir" != "/" ]] || {
    echo "The skill destination is unsafe." >&2
    return 1
  }
  installed_dir="$skills_dir/local-browser-bridge"

  [[ -f "$source_dir/SKILL.md" \
      && -f "$source_dir/references/transport.md" \
      && -f "$source_dir/references/browser.md" \
      && -f "$source_dir/references/computer.md" \
      && -f "$source_dir/references/http.md" ]] || {
    echo "The source skill is incomplete." >&2
    return 1
  }
  actual_inventory="$(cd "$source_dir" && find . -mindepth 1 -print | LC_ALL=C sort)"
  expected_inventory="$(printf '%s\n' \
    ./SKILL.md \
    ./agents \
    ./agents/openai.yaml \
    ./references \
    ./references/browser.md \
    ./references/computer.md \
    ./references/http.md \
    ./references/transport.md | LC_ALL=C sort)"
  [[ "$actual_inventory" == "$expected_inventory" ]] || {
    echo "The source skill inventory is not the exact reviewed set." >&2
    return 1
  }
  if find "$source_dir" -type l -print -quit | grep -q .; then
    echo "The source skill must not contain symbolic links." >&2
    return 1
  fi

  if [[ "$check_only" -eq 1 ]]; then
    [[ -d "$installed_dir" && ! -L "$installed_dir" ]] || {
      echo "Local Browser Bridge skill is not installed at $installed_dir" >&2
      return 1
    }
    if ! diff -qr "$source_dir" "$installed_dir" >/dev/null; then
      echo "Installed Local Browser Bridge skill differs from this source checkout." >&2
      return 1
    fi
    echo "Local Browser Bridge skill verified at $installed_dir"
    return
  fi

  mkdir -p "$skills_dir"
  [[ -d "$skills_dir" && ! -L "$skills_dir" ]] || {
    echo "The skills directory must be an ordinary directory: $skills_dir" >&2
    return 1
  }
  if [[ -e "$installed_dir" || -L "$installed_dir" ]]; then
    if [[ -d "$installed_dir" && ! -L "$installed_dir" ]] \
        && diff -qr "$source_dir" "$installed_dir" >/dev/null; then
      echo "Local Browser Bridge skill is already current at $installed_dir"
      return
    fi
    echo "Refusing to replace a different existing skill at $installed_dir" >&2
    return 1
  fi

  stage_parent="$(mktemp -d "$skills_dir/.local-browser-bridge-install.XXXXXX")"
  staged_skill="$stage_parent/local-browser-bridge"
  trap 'rm -rf "$stage_parent"' RETURN
  mkdir "$staged_skill"
  cp -R "$source_dir/." "$staged_skill/"
  diff -qr "$source_dir" "$staged_skill" >/dev/null
  mv "$staged_skill" "$installed_dir"
  rmdir "$stage_parent"
  trap - RETURN
  echo "Installed Local Browser Bridge skill at $installed_dir"
}

self_test() {
  local scratch skills_dir installed_file
  scratch="$(mktemp -d)"
  trap 'rm -rf "$scratch"' EXIT
  skills_dir="$scratch/skills"
  "$0" --destination "$skills_dir" >/dev/null
  "$0" --destination "$skills_dir" --check >/dev/null
  installed_file="$skills_dir/local-browser-bridge/SKILL.md"
  printf '\n' >> "$installed_file"
  if "$0" --destination "$skills_dir" --check >/dev/null 2>&1; then
    echo "The installer self-test accepted a modified installation." >&2
    return 1
  fi
  if "$0" --destination "$skills_dir" >/dev/null 2>&1; then
    echo "The installer self-test replaced a modified installation." >&2
    return 1
  fi
  rm -rf "$scratch"
  trap - EXIT
  echo "Agent skill installer self-test passed."
}

if [[ "${1:-}" == "--self-test" ]]; then
  [[ "$#" -eq 1 ]] || { usage >&2; exit 2; }
  self_test
  exit 0
fi

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --target)
      [[ "$#" -ge 2 ]] || { usage >&2; exit 2; }
      target="$2"
      shift 2
      ;;
    --destination)
      [[ "$#" -ge 2 ]] || { usage >&2; exit 2; }
      destination="$2"
      shift 2
      ;;
    --check)
      check_only=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage >&2
      exit 2
      ;;
  esac
done

install_or_check
