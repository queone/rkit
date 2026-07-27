# rkit

`rkit` provides small, standalone Rust command-line utilities. Its first
binary, `tree`, prints a lightweight view of a directory hierarchy.

## Why

Use `rkit` when the platform `tree` utility is unavailable or when a compact,
cross-platform implementation is preferable. The package uses only the Rust
standard library.

## Install

Install the stable Rust toolchain, then run:

```bash
./build.sh
```

The canonical build formats, lints, tests, and release-builds the package in a
temporary Cargo target outside the repository. After validation succeeds, it
installs every package binary under `${CARGO_HOME:-$HOME/.cargo}/bin`. It
leaves no repository-local `target/` directory.

## Tree

```text
tree v1.0.3
Directory tree printer — https://github.com/queone/rkit
Usage
  tree [options] [directory]

  Options can appear before or after directory operands. The last directory
  operand is used. Use -- before a directory whose name begins with a dash.

Options
  -f, --full-path  Show each file's path joined to the directory operand
  -v, --version    Print version and exit
  -h, -?, --help   Show this help message and exit
  --               End option parsing
```

The root operand defaults to `.` and is not printed. Entries are ordered by
valid UTF-8 filename, dot-prefixed entries are included, and directory
symlinks are printed without being followed. A filename that is not valid
UTF-8 stops the command with recovery guidance.

An unreadable requested root stops the command. An unreadable descendant
directory remains visible in the tree, its contents are skipped, and a warning
on standard error explains how to include it. Other readable entries continue
printing and the command exits successfully.

`--full-path` preserves the Go utility's historical behavior: it displays the
lexically cleaned path joined to the supplied root. A relative root produces a
relative displayed path; an absolute root produces an absolute displayed path.

Names use the original 256-color palette when standard output is a compatible
terminal. Set `NO_COLOR` to any non-empty value, use `TERM=dumb`, or redirect
output to receive plain text.

Examples:

```bash
tree
tree -f /path/to/directory
tree /path/to/directory --full-path
tree -- -directory
```

## Governance

This repo is governed by an explicit session-entry contract for AI coding agents — see [`governa/operator-contract-rationale.md`](governa/operator-contract-rationale.md) for the design reasoning and [`AGENTS.md`](AGENTS.md) for the operational rules.
