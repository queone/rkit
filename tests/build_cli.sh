#!/usr/bin/env bash
# Build-command regression tests. This file is sourced only in subprocesses.
set -u

test_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
. "$test_root/build.sh"
export NO_COLOR=1
_color_init

test_count=0

fail() {
  printf 'build CLI test failed: %s\n' "$1" >&2
  exit 1
}

assert_contains() {
  case "$1" in
  *"$2"*) ;;
  *) fail "expected $(_quote "$2") in $(_quote "$1")" ;;
  esac
}

assert_equal() {
  [ "$1" = "$2" ] || fail "expected $(_quote "$2"), got $(_quote "$1")"
}

pass() {
  test_count=$((test_count + 1))
}

test_build_parser() {
  local output rc
  output=$(
    build_run() { printf 'route:%s\n' "$*"; }
    build_main tree dos2unix tree --verbose
  ) || fail 'normalized build parser rejected valid targets'
  assert_contains "$output" 'selected utilities: dos2unix tree'
  assert_contains "$output" 'route:1 dos2unix tree'

  output=$(
    build_run() { printf 'route:%s\n' "$*"; }
    build_main -v tree
  ) || fail 'verbose-before-target parser rejected valid input'
  assert_contains "$output" 'route:1 tree'

  output=$(
    build_run() { printf 'route:%s\n' "$*"; }
    build_main tree -v dos2unix
  ) || fail 'verbose-between-targets parser rejected valid input'
  assert_contains "$output" 'route:1 dos2unix tree'

  output=$(
    build_run() { printf 'route:%s\n' "$*"; }
    build_main dos2unix tree
  ) || fail 'reverse-order parser rejected valid targets'
  assert_contains "$output" 'selected utilities: dos2unix tree'
  assert_contains "$output" 'route:0 dos2unix tree'

  set +e
  output=$(build_main Tree 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 2
    assert_contains "$output" 'available utilities: brew-update dos2unix repoctl tree'

  set +e
  output=$(build_main tree,dos2unix 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 2
    assert_contains "$output" 'available utilities: brew-update dos2unix repoctl tree'

  for help_arg in -h '-?' --help; do
    output=$(build_main "$help_arg") || fail "$help_arg help failed"
    assert_contains "$output" 'Usage: build [utility ...] [-v|--verbose]'

    set +e
    output=$(build_main tree "$help_arg" 2>&1)
    rc=$?
    set -e
    assert_equal "$rc" 2
    assert_contains "$output" 'help flags must be used by themselves'
  done
  pass
}

test_main_dispatch() {
  local output
  output=$(
    prep_main() { printf 'prep:%s\n' "$*"; }
    rel_main() { printf 'release:%s\n' "$*"; }
    build_main() { printf 'build:%s\n' "$*"; }
    main prep -n v9.9.9 message
    main v9.9.9 message
    main tree --verbose
  ) || fail 'main dispatch failed'
  assert_contains "$output" 'prep:-n v9.9.9 message'
  assert_contains "$output" 'release:v9.9.9 message'
  assert_contains "$output" 'build:tree --verbose'
  pass
}

test_scoped_command_routing() {
  local output
  output=$(
    _load_bin_targets() { _bin_targets=(dos2unix tree); }
    _run_build_cli_tests() { return 0; }
    _cargo_install_root() { printf '%s' "${TMPDIR:-/tmp}/rkit-build-test-cargo"; }
    _run_cargo() {
      shift 2
      printf 'cargo:%s\n' "$*"
    }
    _run_cargo_install() {
      shift 3
      printf 'cargo:%s\n' "$*"
    }
    _cargo_target="${TMPDIR:-/tmp}/rkit-build-test-target"
    _build_scoped_phases 1 tree
  ) || fail 'scoped command routing failed'
  assert_contains "$output" 'cargo:cargo clippy --verbose --all-features --lib --bin tree --test tree_cli'
  assert_contains "$output" 'cargo:cargo test --verbose --all-features --lib --bin tree --test tree_cli'
  assert_contains "$output" 'cargo:cargo build --verbose --release --bin tree'
  assert_contains "$output" 'cargo:cargo install --verbose --path . --no-track --force --bin tree'
  case "$output" in
  *dos2unix*) fail 'scoped tree routing included dos2unix' ;;
  esac
  pass
}

test_individual_install_routing() {
  local output
  output=$(
    _load_bin_targets() { _bin_targets=(brew-update dos2unix tree); }
    _validate_utility_versions() { :; }
    _run_build_cli_tests() { return 0; }
    _cargo_install_root() { printf '%s' "${TMPDIR:-/tmp}/rkit-build-test-cargo"; }
    _run_cargo() {
      shift 2
      printf 'cargo:%s\n' "$*"
    }
    _run_cargo_install() {
      shift 3
      printf 'cargo:%s\n' "$*"
    }
    _cargo_target="${TMPDIR:-/tmp}/rkit-build-test-target"
    _build_all_phases 1 1
  ) || fail 'individual install routing failed'
  assert_contains "$output" '==> Building and installing brew-update'
  assert_contains "$output" '==> Building and installing dos2unix'
  assert_contains "$output" '==> Building and installing tree'
  assert_contains "$output" 'cargo:cargo install --verbose --path . --bin brew-update --force'
  assert_contains "$output" 'cargo:cargo install --verbose --path . --bin dos2unix --force'
  assert_contains "$output" 'cargo:cargo install --verbose --path . --bin tree --force'
  case "$output" in
  *'==> Building and installing tree'*'==> Building and installing brew-update'*)
    fail 'full install phases were not sorted' ;;
  esac
  pass
}

test_colored_install_name() {
  local output
  output=$(
    NO_COLOR=
    TERM=xterm-256color
    COLORTERM=truecolor
    GOVERNA_FORCE_TTY=1
    _color_init
    _load_bin_targets() { _bin_targets=(brew-update dos2unix tree); }
    _validate_utility_versions() { :; }
    _run_build_cli_tests() { return 0; }
    _cargo_install_root() { printf '%s' "${TMPDIR:-/tmp}/rkit-build-test-cargo"; }
    _run_cargo() {
      shift 2
      printf 'cargo:%s\n' "$*"
    }
    _run_cargo_install() {
      shift 3
      printf 'cargo:%s\n' "$*"
    }
    _cargo_target="${TMPDIR:-/tmp}/rkit-build-test-target"
    _build_all_phases 0 1
  ) || fail 'colored install name failed'
  assert_contains "$output" $'\033[38;5;34mtree\033[0m'
  pass
}

test_quiet_install_status() {
  local output path
  path="${TMPDIR:-/tmp}/rkit-quiet-install-$PPID-$$"
  rm -f -- "$path"
  fake_install() {
    printf 'Installing rkit v1.4.0\n'
    printf 'Finished `release` profile\n'
    printf 'Replacing /tmp/tree\n'
    printf 'Replaced package rkit\n'
  }
  output=$(_run_cargo_install tree 1.4.0 "$path" fake_install) ||
    fail 'quiet install status failed'
  assert_contains "$output" "Installing $path v1.4.0"
  case "$output" in
  *'Installing rkit'*|*'Finished '*|*'Replacing /tmp/tree'*|*'Replaced package'*)
    fail 'Cargo install progress was not suppressed' ;;
  esac
  pass
}

test_release_hint() {
  local output
  output=$(
    _require_cargo() { return 0; }
    _run_isolated() { "$@"; }
    _build_all_phases() { printf 'build:%s\n' "$*"; }
    _cargo_version_info() { printf '1.4.0\n'; }
    build_run 0
  ) || fail 'release hint failed'
  assert_contains "$output" '==> To release, run:'
  assert_contains "$output" './build.sh v1.4.1 "<release message>"'
  pass
}

test_utility_version_validation() {
  local fixture output rc
  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-build-version.XXXXXX") ||
    fail 'could not create utility-version fixture'
  mkdir -p "$fixture/src"
  printf 'pub const PROGRAM_VERSION: &str = "1.2.3";\n' >"$fixture/src/tree.rs"
  output=$(cd "$fixture" && _read_utility_version tree && printf '%s' "$_utility_version_value") ||
    fail 'valid utility version was rejected'
  assert_equal "$output" '1.2.3'

  printf 'const PROGRAM_VERSION: &str = "01.2.3";\n' >"$fixture/src/tree.rs"
  set +e
  output=$(cd "$fixture" && _read_utility_version tree 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'malformed PROGRAM_VERSION'

  printf 'const PROGRAM_VERSION: &str = "1.2.3";\nconst PROGRAM_VERSION: &str = "1.2.4";\n' >"$fixture/src/tree.rs"
  set +e
  output=$(cd "$fixture" && _read_utility_version tree 2>&1)
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'duplicate PROGRAM_VERSION'

  rm -rf -- "$fixture"
  pass
}

test_release_prep_full_build_routing() {
  local output rc
  set +e
  output=$(
    _validate_release_inputs() { return 0; }
    _validate_git_state() { return 0; }
    _cargo_version_info() { printf '1.1.0\n'; }
    _ac_refs() { printf ''; }
    _matching_ac_files() { printf ''; }
    _require_cargo() { return 0; }
    _run_isolated() { "$@"; }
    _build_all_phases() {
      printf 'all:%s\n' "$*"
      return 42
    }
    prep_run 0 0 v1.1.1 message
  )
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'all:0 0'
  pass
}

test_manifest_preflight_failures() {
  local fixture output rc
  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-build-manifest.XXXXXX") ||
    fail 'could not create manifest fixture'
  mkdir -p "$fixture/src/bin" "$fixture/tests"
  printf 'fn main() {}\n' >"$fixture/src/bin/tree.rs"
  {
    printf '[package]\nname = "fixture"\nversion = "1.0.0"\n'
    printf '[[bin]]\nname = "tree"\npath = "src/bin/tree.rs"\n'
  } >"$fixture/Cargo.toml"
  set +e
  output=$(cd "$fixture" && _load_bin_targets 2>&1)
  rc=$?
  set -e
  rm -rf -- "$fixture"
  assert_equal "$rc" 1
  assert_contains "$output" 'missing "tests/tree_cli.rs"'

  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-build-manifest.XXXXXX") ||
    fail 'could not create path fixture'
  mkdir -p "$fixture/src/bin" "$fixture/tests"
  printf 'fn main() {}\n' >"$fixture/src/bin/tree.rs"
  printf 'test\n' >"$fixture/tests/tree_cli.rs"
  {
    printf '[package]\nname = "fixture"\nversion = "1.0.0"\n'
    printf '[[bin]]\nname = "tree"\npath = "src/tree-main.rs"\n'
  } >"$fixture/Cargo.toml"
  set +e
  output=$(cd "$fixture" && _load_bin_targets 2>&1)
  rc=$?
  set -e
  rm -rf -- "$fixture"
  assert_equal "$rc" 1
  assert_contains "$output" 'expected path "src/bin/tree.rs"'

  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-build-manifest.XXXXXX") ||
    fail 'could not create duplicate fixture'
  mkdir -p "$fixture/src/bin" "$fixture/tests"
  printf 'fn main() {}\n' >"$fixture/src/bin/tree.rs"
  printf 'test\n' >"$fixture/tests/tree_cli.rs"
  {
    printf '[package]\nname = "fixture"\nversion = "1.0.0"\n'
    printf '[[bin]]\nname = "tree"\npath = "src/bin/tree.rs"\n'
    printf '[[bin]]\nname = "tree"\npath = "src/bin/other.rs"\n'
  } >"$fixture/Cargo.toml"
  set +e
  output=$(cd "$fixture" && _load_bin_targets 2>&1)
  rc=$?
  set -e
  rm -rf -- "$fixture"
  assert_equal "$rc" 1
  assert_contains "$output" 'duplicate name "tree"'

  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-build-manifest.XXXXXX") ||
    fail 'could not create duplicate-path fixture'
  mkdir -p "$fixture/src/bin" "$fixture/tests"
  printf 'fn main() {}\n' >"$fixture/src/bin/tree.rs"
  printf 'test\n' >"$fixture/tests/tree_cli.rs"
  {
    printf '[package]\nname = "fixture"\nversion = "1.0.0"\n'
    printf '[[bin]]\nname = "tree"\npath = "src/bin/tree.rs"\n'
    printf '[[bin]]\nname = "other"\npath = "src/bin/tree.rs"\n'
  } >"$fixture/Cargo.toml"
  set +e
  output=$(cd "$fixture" && _load_bin_targets 2>&1)
  rc=$?
  set -e
  rm -rf -- "$fixture"
  assert_equal "$rc" 1
  assert_contains "$output" 'duplicate path "src/bin/tree.rs"'

  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-build-manifest.XXXXXX") ||
    fail 'could not create incomplete-table fixture'
  {
    printf '[package]\nname = "fixture"\nversion = "1.0.0"\n'
    printf '[[bin]]\nname = "tree"\n'
  } >"$fixture/Cargo.toml"
  set +e
  output=$(cd "$fixture" && _load_bin_targets 2>&1)
  rc=$?
  set -e
  rm -rf -- "$fixture"
  assert_equal "$rc" 1
  assert_contains "$output" 'missing a literal path'

  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-build-manifest.XXXXXX") ||
    fail 'could not create missing-binary fixture'
  mkdir -p "$fixture/tests"
  printf 'test\n' >"$fixture/tests/tree_cli.rs"
  {
    printf '[package]\nname = "fixture"\nversion = "1.0.0"\n'
    printf '[[bin]]\nname = "tree"\npath = "src/bin/tree.rs"\n'
  } >"$fixture/Cargo.toml"
  set +e
  output=$(cd "$fixture" && _load_bin_targets 2>&1)
  rc=$?
  set -e
  rm -rf -- "$fixture"
  assert_equal "$rc" 1
  assert_contains "$output" '"src/bin/tree.rs" is not a regular file'

  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-build-manifest.XXXXXX") ||
    fail 'could not create empty manifest fixture'
  printf '[package]\nname = "fixture"\nversion = "1.0.0"\n' >"$fixture/Cargo.toml"
  set +e
  output=$(cd "$fixture" && _load_bin_targets 2>&1)
  rc=$?
  set -e
  rm -rf -- "$fixture"
  assert_equal "$rc" 1
  assert_contains "$output" 'no explicit [[bin]] targets'
  pass
}

test_release_decline_has_no_mutations() {
  local fixture log output rc
  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-release-stub.XXXXXX") ||
    fail 'could not create release fixture'
  log="$fixture/git-mutations"

  set +e
  rel_main invalid message >/dev/null 2>&1
  rc=$?
  set -e
  assert_equal "$rc" 1
  set +e
  rel_main v9.9.9 >/dev/null 2>&1
  rc=$?
  set -e
  assert_equal "$rc" 2
  set +e
  output=$(
    git() {
      if [ "$1" = rev-parse ] && [ "${2:-}" = -q ]; then return 0; fi
      return 0
    }
    { _validate_git_state release v9.9.9; } 2>&1
  )
  rc=$?
  set -e
  assert_equal "$rc" 1
  assert_contains "$output" 'tag v9.9.9 already exists'

  set +e
  output=$(
    git() {
      if [ "$1" = rev-parse ]; then
        [ "${2:-}" = -q ] && return 1
        return 0
      fi
      if [ "$1" = status ]; then
        printf ' M fixture\n'
        return 0
      fi
      if [ "$1" = add ] || [ "$1" = commit ] ||
        [ "$1" = tag ] || [ "$1" = push ]; then
        printf '%s\n' "$*" >>"$log"
        return 0
      fi
      return 0
    }
    { printf 'n\n' | rel_run v9.9.9 message; } 2>&1
  )
  rc=$?
  set -e
  [ "$rc" -ne 0 ] || fail 'declined release succeeded'
  [ ! -s "$log" ] || fail 'declined release attempted a git mutation'
  rm -rf -- "$fixture"
  pass
}

test_release_prep_dry_run_has_no_writes() {
  local fixture before_status before_hashes before_head before_tags
  local after_status after_hashes after_head after_tags output
  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-prep-dry.XXXXXX") ||
    fail 'could not create prep dry-run fixture'
  mkdir -p "$fixture/governa"
  printf '[package]\nname = "fixture"\nversion = "1.0.0"\n' >"$fixture/Cargo.toml"
  printf '# lock\n' >"$fixture/Cargo.lock"
  printf '# Changelog\n\n| Version | Summary |\n|---------|---------|\n| Unreleased | |\n' \
    >"$fixture/CHANGELOG.md"
  printf '# Plan\n' >"$fixture/plan.md"
  printf '# AC99 Fixture\n' >"$fixture/governa/ac99-fixture.md"
  (
    cd "$fixture" || exit 1
    git init -q
    git config user.name fixture
    git config user.email fixture@example.invalid
    git add .
    git commit -qm fixture
    printf 'dirty\n' >>plan.md
  ) || fail 'could not initialize prep dry-run fixture'

  before_status=$(git -C "$fixture" status --short)
  before_hashes=$(cd "$fixture" && cksum Cargo.toml Cargo.lock CHANGELOG.md plan.md governa/ac99-fixture.md)
  before_head=$(git -C "$fixture" rev-parse HEAD)
  before_tags=$(git -C "$fixture" tag --list)
  output=$(cd "$fixture" && prep_run 1 0 v1.0.1 'AC99: dry run') ||
    fail 'release prep dry-run failed'
  after_status=$(git -C "$fixture" status --short)
  after_hashes=$(cd "$fixture" && cksum Cargo.toml Cargo.lock CHANGELOG.md plan.md governa/ac99-fixture.md)
  after_head=$(git -C "$fixture" rev-parse HEAD)
  after_tags=$(git -C "$fixture" tag --list)

  assert_equal "$after_status" "$before_status"
  assert_equal "$after_hashes" "$before_hashes"
  assert_equal "$after_head" "$before_head"
  assert_equal "$after_tags" "$before_tags"
  assert_contains "$output" './build.sh v1.0.1 "AC99: dry run"'
  rm -rf -- "$fixture"
  pass
}

test_release_prep_preserves_utility_versions() {
  local fixture before after output
  fixture=$(mktemp -d "${TMPDIR:-/tmp}/rkit-prep-version.XXXXXX") ||
    fail 'could not create prep version fixture'
  mkdir -p "$fixture/governa" "$fixture/src"
  printf '[package]\nname = "fixture"\nversion = "1.0.0"\n' >"$fixture/Cargo.toml"
  printf 'pub const PROGRAM_VERSION: &str = "9.8.7";\n' >"$fixture/src/tree.rs"
  printf '# Changelog\n\n| Version | Summary |\n|---------|---------|\n| Unreleased | |\n' \
    >"$fixture/CHANGELOG.md"
  (
    cd "$fixture" || exit 1
    git init -q
    git config user.name fixture
    git config user.email fixture@example.invalid
    git add .
    git commit -qm fixture
  ) || fail 'could not initialize prep version fixture'
  before=$(cksum "$fixture/src/tree.rs")
  output=$(cd "$fixture" && _refresh_cargo_lock() { return 0; }; prep_run 0 1 v1.0.1 release) ||
    fail 'release prep version fixture failed'
  after=$(cksum "$fixture/src/tree.rs")
  assert_equal "$before" "$after"
  assert_contains "$(cat "$fixture/Cargo.toml")" 'version = "1.0.1"'
  rm -rf -- "$fixture"
  pass
}

test_build_parser
test_main_dispatch
test_scoped_command_routing
test_individual_install_routing
test_colored_install_name
test_quiet_install_status
test_release_hint
test_utility_version_validation
test_release_prep_full_build_routing
test_manifest_preflight_failures
test_release_decline_has_no_mutations
test_release_prep_dry_run_has_no_writes
test_release_prep_preserves_utility_versions

printf 'build CLI tests: %d passed\n' "$test_count"
