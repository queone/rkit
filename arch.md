# rkit Architecture

## Purpose

Provide small, standalone Rust command-line utilities with deterministic
behavior and no third-party runtime dependencies.

## System Summary

The root Cargo package builds the `tree` and `dos2unix` binaries. Each binary
keeps process streams and exit codes at its entrypoint and delegates testable
behavior to a namespaced `rkit` library module. The package reads the local
filesystem and writes only to requested regular files, standard output, and
standard error.

## Current Platform

- Rust

## Major Components

- `src/bin/tree.rs`: `tree` process streams and exit-code boundary
- `src/bin/dos2unix.rs`: `dos2unix` process streams and exit-code boundary
- `src/tree.rs`: tree parsing, traversal, path handling, and rendering
- `src/dos2unix.rs`: CRLF parsing, preview, conversion, and diagnostics
- `src/color.rs`: shared terminal color policy
- `src/lib.rs`: narrow package-binary library boundary
- `tests/cli.rs`: compiled-binary acceptance coverage
- `tests/dos2unix_cli.rs`: compiled `dos2unix` behavior and file-effect coverage
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
- `src/color.rs`: shared terminal color detection and rendering
- `tests/cli.rs`: end-to-end `tree` tests
- `tests/dos2unix_cli.rs`: end-to-end `dos2unix` tests
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

## Conventions

- Keep process I/O in `src/bin/`.
- Keep testable behavior in namespaced source modules.
- Share terminal detection through `src/color.rs`.
- Derive displayed versions from Cargo package metadata.
- Add no third-party dependency without an approved AC.
