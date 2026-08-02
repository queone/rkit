# rkit Architecture

## Purpose

Provide small, standalone Rust command-line utilities with deterministic
behavior and no third-party runtime dependencies.

## System Summary

The root Cargo package builds the `tree`, `dos2unix`, `brew-update`, and
`repoctl` binaries. Each binary
keeps process streams and exit codes at its entrypoint and delegates testable
behavior to a namespaced `rkit` library module. The package reads the local
filesystem and writes to standard output and standard error; file mutation is
limited to utilities that explicitly request it, including `repoctl`'s
delegated Git clone/pull and repository build operations.

## Current Platform

- Rust

## Major Components

- `src/bin/tree.rs`: `tree` process streams and exit-code boundary
- `src/bin/dos2unix.rs`: `dos2unix` process streams and exit-code boundary
- `src/bin/repoctl.rs`: `repoctl` process streams and exit-code boundary
- `src/tree.rs`: tree parsing, traversal, path handling, and rendering
- `src/dos2unix.rs`: CRLF parsing, preview, conversion, and diagnostics
- `src/repoctl.rs`: repository discovery, operation execution, sorting, rendering, and diagnostics
- `src/color.rs`: shared terminal color policy
- `src/lib.rs`: narrow package-binary library boundary
- `tests/tree_cli.rs`: compiled `tree` behavior coverage
- `tests/dos2unix_cli.rs`: compiled `dos2unix` behavior and file-effect coverage
- `tests/repoctl_cli.rs`: `repoctl` command, operation, sorting, and output coverage
- `tests/build_cli.sh`: build routing and release-safety coverage
- `build.sh`: isolated Cargo validation and package-binary installation

## Core Files

- `AGENTS.md`: base governance contract
- `plan.md`: prioritized roadmap and approved direction
- `build.sh`: self-contained build / release-prep / release script (Bash 3.2+, no external tools)
- `Cargo.toml`: Rust package, library, and binary declarations
- `src/lib.rs`: package-binary module boundary
- `src/bin/`: process entrypoints
- `src/tree.rs`: reusable `tree` behavior
- `src/dos2unix.rs`: reusable `dos2unix` behavior
- `src/repoctl.rs`: reusable `repoctl` behavior
- `src/color.rs`: shared terminal color detection and rendering
- `tests/tree_cli.rs`: end-to-end `tree` tests
- `tests/dos2unix_cli.rs`: end-to-end `dos2unix` tests
- `tests/repoctl_cli.rs`: end-to-end `repoctl` tests
- `tests/build_cli.sh`: Bash 3.2-compatible build and release routing tests
- `governa/development-cycle.md`: workflow from roadmap through release
- `governa/ac-template.md`: acceptance-criteria template for new work
- `governa/build-release.md`: build, test, and release rules

## Data And Control Flow

### tree

1. Parse arguments until a help or version terminal flag, or until traversal
   options and the last directory operand are settled.
2. Read and sort each directory without following symlinks.
3. Gather readable entries and collect warnings for unreadable descendants.
4. Fail without output when the requested root or a filename is unsupported.
5. Render plain or terminal-aware 256-color output after traversal completes.
6. Return complete output, non-fatal warnings, or a contextual fatal
   diagnostic and exact exit code to the process entrypoint.

### dos2unix

1. Parse one regular-file operand, conversion options, and terminal flags.
2. Follow a symbolic-link operand through one open file handle.
3. Validate the opened target as a regular file.
4. Read and transform all bytes without UTF-8 decoding.
5. Return a complete preview before process output or rewrite the same file
   handle after the initial read succeeds.
6. Preserve inode identity and expose a contextual diagnostic for every
   failure, including the partial-write risk after truncation.

### repoctl

1. Parse the command and discover local repositories, or resolve the owner
   repository list for clone.
2. Resolve each Origin before operation execution and sort the work by Origin.
3. Complete status and pull operations before rendering their colored Repo,
   Origin, and result rows, followed only by retained uncolored details.
4. Render and flush build and clone processing rows before starting work.
5. Stream build and clone stdout and stderr as indented details, then render one
   colored indented final status.
6. Inject `GOVERNA_FORCE_TTY=1` only for a color-enabled governed build when
   the parent environment does not define it; preserve inherited values.
7. Process repositories sequentially, retaining the aggregate failure state.
8. Return the aggregate operation exit code after all selected repositories
   have been attempted.

## Architecture Notes

- Use only the Rust standard library.
- Keep the public library surface limited to package-binary run, output, and
  error boundaries.
- Accept valid UTF-8 filenames and fail explicitly on unsupported names.
- Measure alignment by Unicode scalar count for compatibility with the Go
  implementation.
- Lexically clean displayed joined paths without filesystem canonicalization.
- Build in an external temporary Cargo target and install binaries under
  `${CARGO_HOME:-$HOME/.cargo}/bin`.
- Discover utility targets from explicit Cargo `[[bin]]` tables whose paths
  follow `src/bin/<utility>.rs`.
- Map each utility to `tests/<utility>_cli.rs`.
- Keep full builds and release validation package-wide.
- Scope binary checks, integration tests, release artifacts, and installation
  when space-separated utility names are supplied.
- Keep shared-library tests and formatting package-wide during scoped builds.
- Use tracked Cargo installation for full builds.
- Use untracked forced Cargo installation for explicitly selected binaries so
  unselected binaries and Cargo tracking metadata remain unchanged.

## Conventions

- Keep process I/O in `src/bin/`.
- Keep testable behavior in namespaced source modules.
- Share terminal detection through `src/color.rs`.
- Derive displayed versions from Cargo package metadata.
- Add no third-party dependency without an approved AC.
