#!/usr/bin/env bash
# build.sh — canonical Rust build, release-prep, and release tooling.
# Targets Bash 3.2+ and delegates language work to Cargo.
set -euo pipefail

_trim() {
  local s="$1"
  s="${s#"${s%%[![:space:]]*}"}"
  s="${s%"${s##*[![:space:]]}"}"
  printf '%s' "$s"
}

_byte_len() { LC_ALL=C printf '%s' "$1" | LC_ALL=C wc -c | tr -d ' '; }

_quote() {
  printf '"%s"' "$(printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g')"
}

_require_cargo() {
  if ! command -v cargo >/dev/null 2>&1; then
    printf '%s\n' \
      'build: cargo is required; install the Rust toolchain from https://rustup.rs/ and retry' >&2
    return 1
  fi
}

_path_is_within() {
  case "$1/" in
  "$2/"*) return 0 ;;
  *) return 1 ;;
  esac
}

_cargo_target=''
_repo_root=''

_cleanup_cargo_target() {
  local target="${_cargo_target:-}"
  [ -n "$target" ] || return 0
  case "$target" in
  / | "${HOME:-}") return 1 ;;
  esac
  [ -n "$_repo_root" ] || return 1
  _path_is_within "$target" "$_repo_root" && return 1
  case "$(basename "$target")" in
  governa-rust-target.*) ;;
  *) return 1 ;;
  esac
  rm -rf -- "$target" || return 1
  _cargo_target=''
}

_cargo_signal() {
  local status="$1"
  trap - HUP INT TERM
  if ! _cleanup_cargo_target; then
    printf '%s\n' \
      "build: cleanup failed for Cargo target ${_cargo_target:-unknown}; remove it manually" >&2
  fi
  exit "$status"
}

_create_cargo_target() {
  local parent candidate resolved home=''
  _repo_root=$(pwd -P) || {
    printf '%s\n' 'build: resolve repository root: failed; check directory access' >&2
    return 1
  }
  parent="${TMPDIR:-/tmp}"
  if ! resolved=$(cd "$parent" 2>/dev/null && pwd -P) ||
     [ ! -w "$resolved" ] ||
     _path_is_within "$resolved" "$_repo_root"; then
    parent=/tmp
    if ! resolved=$(cd "$parent" 2>/dev/null && pwd -P) ||
       [ ! -w "$resolved" ] ||
       _path_is_within "$resolved" "$_repo_root"; then
      printf '%s\n' \
        'build: create Cargo target: no safe temporary directory; set TMPDIR to a writable path outside the repository' >&2
      return 1
    fi
  fi
  candidate=$(mktemp -d "$resolved/governa-rust-target.XXXXXX") || {
    printf '%s\n' \
      'build: create Cargo target: mktemp failed; set TMPDIR to a writable path outside the repository' >&2
    return 1
  }
  candidate=$(cd "$candidate" 2>/dev/null && pwd -P) || {
    rmdir "$candidate" 2>/dev/null || true
    printf '%s\n' 'build: resolve Cargo target: failed; remove the temporary directory manually' >&2
    return 1
  }
  [ -n "${HOME:-}" ] && home=$(cd "$HOME" 2>/dev/null && pwd -P || true)
  case "$candidate" in
  / | "$home")
    printf '%s\n' 'build: unsafe Cargo target path; refusing cleanup' >&2
    return 1
    ;;
  esac
  if _path_is_within "$candidate" "$_repo_root"; then
    printf '%s\n' 'build: Cargo target resolved inside the repository; set a safe TMPDIR' >&2
    return 1
  fi
  case "$(basename "$candidate")" in
  governa-rust-target.*) ;;
  *)
    printf '%s\n' 'build: Cargo target has an unsafe name; refusing cleanup' >&2
    return 1
    ;;
  esac
  _cargo_target="$candidate"
  export CARGO_TARGET_DIR="$_cargo_target"
}

_run_isolated() {
  local rc=0 cleanup_rc=0
  _create_cargo_target || return 1
  trap '_cargo_signal 129' HUP
  trap '_cargo_signal 130' INT
  trap '_cargo_signal 143' TERM
  "$@" || rc=$?
  trap - HUP INT TERM
  _cleanup_cargo_target || cleanup_rc=$?
  if [ "$rc" -ne 0 ]; then return "$rc"; fi
  if [ "$cleanup_rc" -ne 0 ]; then
    printf '%s\n' \
      "build: cleanup failed for Cargo target ${_cargo_target:-unknown}; remove it manually" >&2
    return "$cleanup_rc"
  fi
}

_cargo_install_root() {
  local root parent leaf resolved
  if [ -n "${CARGO_HOME:-}" ]; then
    root="$CARGO_HOME"
  elif [ -n "${HOME:-}" ]; then
    root="$HOME/.cargo"
  else
    printf '%s\n' \
      'build: resolve Cargo install root: set CARGO_HOME or HOME to an external writable directory' >&2
    return 1
  fi
  case "$root" in
  /*) ;;
  *) root="$_repo_root/$root" ;;
  esac
  if _path_is_within "$root" "$_repo_root"; then
    printf '%s\n' \
      'build: Cargo install root resolves inside the repository; set CARGO_HOME outside the repository' >&2
    return 1
  fi
  parent=$(dirname "$root")
  leaf=$(basename "$root")
  mkdir -p "$parent" || {
    printf '%s\n' 'build: create Cargo install parent: failed; check CARGO_HOME or HOME permissions' >&2
    return 1
  }
  resolved=$(cd "$parent" 2>/dev/null && pwd -P) || {
    printf '%s\n' 'build: resolve Cargo install root: failed; check CARGO_HOME or HOME' >&2
    return 1
  }
  root="$resolved/$leaf"
  if [ -e "$root" ]; then
    root=$(cd "$root" 2>/dev/null && pwd -P) || {
      printf '%s\n' 'build: resolve Cargo install root: failed; check CARGO_HOME or HOME' >&2
      return 1
    }
  fi
  if _path_is_within "$root" "$_repo_root"; then
    printf '%s\n' \
      'build: Cargo install root resolves inside the repository; set CARGO_HOME outside the repository' >&2
    return 1
  fi
  printf '%s' "$root"
}

_run_cargo() {
  local step="$1" component="$2"
  shift 2
  printf '    %s\n' "$*"
  local rc=0
  "$@" || rc=$?
  if [ "$rc" -eq 0 ]; then return 0; fi
  case "$component" in
  rustfmt)
    printf '%s\n' \
      "$step failed; if rustfmt is unavailable, run: rustup component add rustfmt" >&2
    ;;
  clippy)
    printf '%s\n' \
      "$step failed; if Clippy is unavailable, run: rustup component add clippy" >&2
    ;;
  *)
    printf '%s failed: exit status %d\n' "$step" "$rc" >&2
    ;;
  esac
  return "$rc"
}

build_usage() {
  cat <<'EOF'
Usage: build [-v|--verbose]

  -v, --verbose   show verbose Cargo output
  -h, -?, --help  show this help
EOF
}

_build_phases() {
  local verbose="$1" install="$2" rc=0

  printf '%s\n' '==> Check Rust formatting'
  _run_cargo 'cargo fmt --check' rustfmt cargo fmt --check || return $?

  printf '\n%s\n' '==> Run Clippy'
  if [ "$verbose" -eq 1 ]; then
    _run_cargo 'cargo clippy' clippy \
      cargo clippy --verbose --all-targets --all-features \
      --target-dir "$_cargo_target" -- -D warnings ||
      return $?
  else
    _run_cargo 'cargo clippy' clippy \
      cargo clippy --all-targets --all-features \
      --target-dir "$_cargo_target" -- -D warnings || return $?
  fi

  printf '\n%s\n' '==> Run tests'
  if [ "$verbose" -eq 1 ]; then
    _run_cargo 'cargo test' '' \
      cargo test --verbose --all-targets --all-features \
      --target-dir "$_cargo_target" || return $?
  else
    _run_cargo 'cargo test' '' \
      cargo test --all-targets --all-features \
      --target-dir "$_cargo_target" || return $?
  fi

  printf '\n%s\n' '==> Build release artifacts'
  if [ "$verbose" -eq 1 ]; then
    _run_cargo 'cargo build --release' '' \
      cargo build --verbose --release --target-dir "$_cargo_target" || return $?
  else
    _run_cargo 'cargo build --release' '' \
      cargo build --release --target-dir "$_cargo_target" || return $?
  fi

  [ "$install" -eq 1 ] || return 0
  local install_root
  install_root=$(_cargo_install_root) || return 1
  printf '\n%s\n' '==> Install package binaries'
  if [ "$verbose" -eq 1 ]; then
    _run_cargo 'cargo install package binaries' '' \
      cargo install --verbose --path . --bins --all-features --locked \
      --root "$install_root" --target-dir "$_cargo_target" || {
        rc=$?
        printf '%s\n' \
          'build: install package binaries: declare at least one Cargo binary target and resolve destination conflicts before retrying' >&2
        return "$rc"
      }
  else
    _run_cargo 'cargo install package binaries' '' \
      cargo install --path . --bins --all-features --locked \
      --root "$install_root" --target-dir "$_cargo_target" || {
        rc=$?
        printf '%s\n' \
          'build: install package binaries: declare at least one Cargo binary target and resolve destination conflicts before retrying' >&2
        return "$rc"
      }
  fi
}

build_run() {
  local verbose="$1"
  _require_cargo || return 1
  _run_isolated _build_phases "$verbose" 1
}

_refresh_cargo_lock() {
  _run_cargo 'cargo check' '' \
    cargo check --all-targets --all-features --target-dir "$_cargo_target"
}

build_main() {
  if [ "$#" -eq 1 ]; then
    case "$1" in -h | -\? | --help) build_usage; return 0 ;; esac
  fi
  local verbose=0 arg
  for arg in "$@"; do
    case "$arg" in
    -v | --verbose) verbose=1 ;;
    -h | -\? | --help)
      printf '%s\n' 'help flags must be used by themselves' >&2
      return 2
      ;;
    -*)
      printf 'unsupported option %s; use optional -v or --verbose\n' "$(_quote "$arg")" >&2
      return 2
      ;;
    *)
      printf 'unexpected argument %s; Rust builds do not accept targets\n' "$(_quote "$arg")" >&2
      return 2
      ;;
    esac
  done
  build_run "$verbose"
}

_latest_tag() {
  git tag --list 2>/dev/null | awk '
    /^v[0-9]+\.[0-9]+\.[0-9]+$/ {
      split(substr($0, 2), a, ".")
      if (!found || a[1]+0>ma || (a[1]+0==ma && (a[2]+0>mi ||
          (a[2]+0==mi && a[3]+0>pa)))) {
        ma=a[1]+0; mi=a[2]+0; pa=a[3]+0; tag=$0; found=1
      }
    }
    END { if (found) print tag }'
}

_validate_release_inputs() {
  local prefix="$1" version="$2" message="$3"
  if ! printf '%s' "$version" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
    printf '%s: version must match vMAJOR.MINOR.PATCH: %s\n' \
      "$prefix" "$(_quote "$version")" >&2
    return 1
  fi
  if [ -z "$message" ]; then
    printf '%s: message must be non-empty\n' "$prefix" >&2
    return 1
  fi
  if [ "$(_byte_len "$message")" -gt 80 ]; then
    printf '%s: message must be 80 characters or fewer\n' "$prefix" >&2
    return 1
  fi
}

_validate_git_state() {
  local prefix="$1" version="$2"
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    printf '%s: run from the root of a git work tree\n' "$prefix" >&2
    return 1
  fi
  if git rev-parse -q --verify "refs/tags/$version" >/dev/null 2>&1; then
    printf '%s: tag %s already exists\n' "$prefix" "$version" >&2
    return 1
  fi
}

_cargo_version_info() {
  local manifest="$1"
  if [ ! -f "$manifest" ]; then
    printf '%s\n' 'Cargo.toml is missing'
    return 1
  fi
  awk '
    BEGIN { package=0; packages=0; versions=0; workspace=0 }
    /^[[:space:]]*\[workspace\][[:space:]]*($|#)/ { workspace=1 }
    /^[[:space:]]*\[package\][[:space:]]*($|#)/ {
      package=1; packages++; next
    }
    /^[[:space:]]*\[/ { package=0 }
    package && /^[[:space:]]*version[[:space:]]*=/ {
      versions++
      line=$0
      sub(/^[[:space:]]*version[[:space:]]*=[[:space:]]*/, "", line)
      if (line ~ /^"[0-9]+\.[0-9]+\.[0-9]+"[[:space:]]*(#.*)?$/) {
        value=line
        sub(/^"/, "", value)
        sub(/".*$/, "", value)
        literal=value
      } else {
        invalid=1
      }
    }
    END {
      if (workspace) { print "Cargo workspaces are not supported; use a single root package"; exit 1 }
      if (packages != 1) { print "Cargo.toml must contain exactly one root [package] table"; exit 1 }
      if (versions != 1) { print "root [package] must contain exactly one version key"; exit 1 }
      if (invalid || literal == "") {
        print "root [package] version must be a literal \"MAJOR.MINOR.PATCH\" value"; exit 1
      }
      print literal
    }' "$manifest"
}

_replace_cargo_version() {
  local manifest="$1" version="$2" tmp
  tmp=$(mktemp "${TMPDIR:-/tmp}/cargo-version.XXXXXX")
  awk -v version="$version" '
    BEGIN { package=0; changed=0 }
    /^[[:space:]]*\[package\][[:space:]]*($|#)/ { package=1; print; next }
    /^[[:space:]]*\[/ { package=0 }
    package && !changed && /^[[:space:]]*version[[:space:]]*=/ {
      match($0, /"[0-9]+\.[0-9]+\.[0-9]+"/)
      print substr($0, 1, RSTART-1) "\"" version "\"" substr($0, RSTART+RLENGTH)
      changed=1
      next
    }
    { print }
    END { if (!changed) exit 1 }' "$manifest" >"$tmp" || {
      rm -f "$tmp"
      printf '%s\n' 'prep: update Cargo.toml version: failed' >&2
      return 1
    }
  cat "$tmp" >"$manifest"
  rm -f "$tmp"
}

_insert_changelog_row() {
  local file="$1" version="$2" message="$3" tmp
  [ -f "$file" ] || return 0
  if ! rg -q '^\| Unreleased \|' "$file" 2>/dev/null &&
     ! grep -Eq '^\| Unreleased \|' "$file"; then
    printf 'prep: %s has no Unreleased row\n' "$file" >&2
    return 1
  fi
  if grep -Fq "| $version |" "$file"; then
    printf 'prep: %s already contains version %s\n' "$file" "$version" >&2
    return 1
  fi
  tmp=$(mktemp "${TMPDIR:-/tmp}/changelog.XXXXXX")
  awk -v row="| $version | $message |" '
    { print }
    !done && /^\| Unreleased \|/ { print row; done=1 }' "$file" >"$tmp"
  cat "$tmp" >"$file"
  rm -f "$tmp"
}

_ac_refs() {
  printf '%s' "$1" | grep -oE 'AC[0-9]+' | sed 's/^AC//' | LC_ALL=C sort -n -u || true
}

_matching_ac_files() {
  local refs="$1" file name number
  [ -n "$refs" ] || return 0
  for file in governa/ac[0-9]*-*.md; do
    [ -f "$file" ] || continue
    name=$(basename "$file")
    number=$(printf '%s' "$name" | sed -E 's/^ac([0-9]+)-.*/\1/')
    if printf '%s\n' "$refs" | grep -qx "$number"; then
      printf '%s\n' "$file"
    fi
  done | LC_ALL=C sort
}

_remove_plan_pointers() {
  local refs="$1" tmp line number
  [ -f plan.md ] || return 0
  [ -n "$refs" ] || return 0
  tmp=$(mktemp "${TMPDIR:-/tmp}/plan.XXXXXX")
  while IFS= read -r line || [ -n "$line" ]; do
    number=$(printf '%s' "$line" |
      sed -nE 's/.*→[[:space:]]+governa\/ac([0-9]+)-.*/\1/p')
    if [ -n "$number" ] && printf '%s\n' "$refs" | grep -qx "$number"; then
      printf 'prep: removed plan.md IE line: %s\n' "$(_trim "$line")"
      continue
    fi
    printf '%s\n' "$line" >>"$tmp"
  done <plan.md
  cat "$tmp" >plan.md
  rm -f "$tmp"
}

prep_usage() {
  cat <<'EOF'
prep vX.Y.Z "release message" [--dry-run|-n] [--no-build|-B]

Stages a release by updating the root Cargo package version and Cargo.lock,
inserting a CHANGELOG row, deleting completed AC files, and validating.

  -h, -?, --help  show this help
  --dry-run, -n   print intended writes without modifying the working tree
  --no-build, -B  skip pre-change and post-change ./build.sh validation
EOF
}

prep_run() {
  local dry="$1" nobuild="$2" version="$3" message="$4"
  local stripped="${version#v}" current refs acfiles file
  _validate_release_inputs prep "$version" "$message" || return 1
  _validate_git_state prep "$version" || return 1
  current=$(_cargo_version_info Cargo.toml) || {
    printf 'prep: %s\n' "$current" >&2
    return 1
  }
  refs=$(_ac_refs "$message")
  acfiles=$(_matching_ac_files "$refs")

  if [ "$dry" -eq 1 ]; then
    printf 'version bumps:\n  Cargo.toml [package].version: %s -> %s\n' \
      "$current" "$stripped"
    [ -f Cargo.lock ] && printf '%s\n' '  Cargo.lock: refresh with cargo check'
    [ -f CHANGELOG.md ] && printf 'changelog rows:\n  CHANGELOG.md: %s\n' "$stripped"
    while IFS= read -r file; do
      [ -n "$file" ] && printf 'delete completed AC: %s\n' "$file"
    done <<EOF
$acfiles
EOF
    printf './build.sh %s %s\n' "$version" "$(_quote "$message")"
    return 0
  fi

  if [ "$nobuild" -ne 1 ]; then
    printf '%s\n' 'prep: running pre-change build'
    _require_cargo || return 1
    _run_isolated _build_phases 0 0 || return 1
  fi

  _replace_cargo_version Cargo.toml "$stripped" || return 1
  printf 'prep: updated Cargo.toml [package].version to %s\n' "$stripped"

  _require_cargo || return 1
  printf '%s\n' 'prep: refreshing Cargo.lock'
  _run_isolated _refresh_cargo_lock || {
    printf '%s\n' 'prep: refresh Cargo.lock with cargo check: failed' >&2
    return 1
  }

  _insert_changelog_row CHANGELOG.md "$stripped" "$message" || return 1
  while IFS= read -r file; do
    [ -n "$file" ] || continue
    rm -- "$file"
    printf 'prep: deleted %s\n' "$file"
  done <<EOF
$acfiles
EOF
  _remove_plan_pointers "$refs"

  if [ "$nobuild" -ne 1 ]; then
    printf '%s\n' 'prep: running post-change build'
    ./build.sh || return 1
  fi
  printf './build.sh %s %s\n' "$version" "$(_quote "$message")"
}

prep_main() {
  if [ "$#" -eq 0 ]; then prep_usage; return 0; fi
  if [ "$#" -eq 1 ]; then
    case "$1" in -h | -\? | --help) prep_usage; return 0 ;; esac
  fi
  local dry=0 nobuild=0 positional=() arg
  for arg in "$@"; do
    case "$arg" in
    --dry-run | -n) dry=1 ;;
    --no-build | -B) nobuild=1 ;;
    -h | -\? | --help)
      printf '%s\n' 'help flags must be used by themselves' >&2
      return 2
      ;;
    -*) printf 'unsupported prep option %s\n' "$(_quote "$arg")" >&2; return 2 ;;
    *) positional+=("$arg") ;;
    esac
  done
  if [ "${#positional[@]}" -ne 2 ]; then
    printf '%s\n' 'usage: prep vX.Y.Z "release message"' >&2
    return 2
  fi
  prep_run "$dry" "$nobuild" "$(_trim "${positional[0]}")" \
    "$(_trim "${positional[1]}")"
}

rel_usage() {
  cat <<'EOF'
rel vX.Y.Z "release message"

Commits the working tree, creates the tag, and pushes the tag and branch after
interactive confirmation.
EOF
}

_release_step() {
  local name="$1" completed="$2"
  shift 2
  local out rc=0
  out=$(git "$@" 2>&1) || rc=$?
  if [ "$rc" -eq 0 ]; then
    [ -n "$out" ] && printf '%s\n' "$out"
    return 0
  fi
  printf 'release: %s failed after [%s]: exit status %d: %s\n' \
    "$name" "$completed" "$rc" "$(_trim "$out")" >&2
  printf '%s\n' \
    'release: inspect git status and remote state before retrying any step' >&2
  return 1
}

rel_run() {
  local version="$1" message="$2" answer completed=''
  _validate_release_inputs release "$version" "$message" || return 1
  _validate_git_state release "$version" || return 1
  printf 'release tag: %s\n' "$version"
  printf 'release message: %s\n' "$(_quote "$message")"
  printf '%s\n' 'remote: origin'
  printf '%s\n' 'Files that will be staged (git status):'
  git status --short || return 1
  printf '%s\n' 'plan:'
  printf '%s\n' '- git add .'
  printf -- '- git commit -m %s\n' "$(_quote "$message")"
  printf '%s\n' "- git tag $version"
  printf '%s\n' "- git push origin $version"
  printf '%s\n' '- git push origin'
  printf '%s' 'Review the file list above. Proceed with release? (y/N): '
  IFS= read -r answer || true
  case "$answer" in
  y | Y) ;;
  *) printf '%s\n' 'release aborted' >&2; return 1 ;;
  esac
  _release_step 'git add' "$completed" add . || return 1
  completed='git add'
  _release_step 'git commit' "$completed" commit -m "$message" || return 1
  completed='git add, git commit'
  _release_step 'git tag' "$completed" tag "$version" || return 1
  completed='git add, git commit, git tag'
  _release_step 'git push tag' "$completed" push origin "$version" || return 1
  completed='git add, git commit, git tag, git push tag'
  _release_step 'git push branch' "$completed" push origin || return 1
}

rel_main() {
  if [ "$#" -eq 0 ]; then rel_usage; return 0; fi
  if [ "$#" -eq 1 ]; then
    case "$1" in -h | -\? | --help) rel_usage; return 0 ;; esac
  fi
  if [ "$#" -ne 2 ]; then
    printf '%s\n' 'usage: rel vX.Y.Z "release message"' >&2
    return 2
  fi
  rel_run "$(_trim "$1")" "$(_trim "$2")"
}

main() {
  if [ "${1:-}" = prep ]; then
    shift
    prep_main "$@"
    return $?
  fi
  if [ "$#" -ge 1 ] && printf '%s' "$1" |
     grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$'; then
    rel_main "$@"
    return $?
  fi
  build_main "$@"
}

if [ "${BASH_SOURCE[0]}" = "${0}" ]; then
  main "$@"
fi
