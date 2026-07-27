# rkit Architecture

## Purpose

Provide small, standalone Rust command-line utilities with deterministic
behavior and no third-party runtime dependencies.

## System Summary

The root Cargo package builds the `tree` binary. The binary delegates argument
parsing, traversal, rendering, color selection, and error construction to the
`rkit` library. The package reads the local filesystem and writes only to
standard output and standard error.

## Current Platform

- Rust

## Major Components

- `src/main.rs`: process streams and exit-code boundary
- `src/lib.rs`: CLI parsing, filesystem traversal, path handling, rendering,
  terminal color policy, and contextual diagnostics
- `tests/cli.rs`: compiled-binary acceptance coverage
- `build.sh`: isolated Cargo validation and package-binary installation

## Core Files

- `AGENTS.md`: base governance contract
- `plan.md`: prioritized roadmap and approved direction
- `build.sh`: self-contained build / release-prep / release script (Bash 3.2+, no external tools)
- `Cargo.toml`: Rust package, library, and binary declarations
- `src/lib.rs`: reusable tree behavior
- `src/main.rs`: `tree` process entrypoint
- `tests/cli.rs`: end-to-end CLI tests
- `governa/development-cycle.md`: workflow from roadmap through release
- `governa/ac-template.md`: acceptance-criteria template for new work
- `governa/build-release.md`: build, test, and release rules

## Data And Control Flow

1. Parse arguments until a help or version terminal flag, or until traversal
   options and the last directory operand are settled.
2. Read and sort each directory without following symlinks.
3. Gather readable entries and collect warnings for unreadable descendants.
4. Fail without output when the requested root or a filename is unsupported.
5. Render plain or terminal-aware 256-color output after traversal completes.
6. Return complete output, non-fatal warnings, or a contextual fatal
   diagnostic and exact exit code to the process entrypoint.

## Architecture Notes

- Use only the Rust standard library.
- Accept valid UTF-8 filenames and fail explicitly on unsupported names.
- Measure alignment by Unicode scalar count for compatibility with the Go
  implementation.
- Lexically clean displayed joined paths without filesystem canonicalization.
- Build in an external temporary Cargo target and install binaries under
  `${CARGO_HOME:-$HOME/.cargo}/bin`.

## Conventions

- Keep process I/O in `src/main.rs`.
- Keep testable behavior in `src/lib.rs`.
- Derive displayed versions from Cargo package metadata.
- Add no third-party dependency without an approved AC.
