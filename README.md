# rkit

`rkit` provides small, standalone Rust command-line utilities. It currently
includes `tree` for directory hierarchies, `dos2unix` for previewing or
converting CRLF line endings, `brew-update` for maintaining Homebrew packages
on macOS, and `repoctl` for operating on collections of local Git repositories.

## Why

Use `rkit` when compact, cross-platform implementations of common command-line
tools are preferable. The package uses only the Rust standard library.

## Install

On macOS, first confirm that the Xcode Command Line Tools are active:

```bash
xcode-select --print-path
```

If that command reports that the tools are missing, install them and complete
the system dialog before continuing:

```bash
xcode-select --install
```

Install Rust with the Rust project's canonical rustup installer:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Accept rustup's default toolchain profile. Load Cargo into the current shell,
verify the toolchain, ensure the canonical build components are installed, and
build the package:

```bash
source "$HOME/.cargo/env"
rustup --version
rustc --version
cargo --version
rustup component add rustfmt clippy
./build.sh
```

The canonical build formats, lints, tests, and release-builds the package in a
temporary Cargo target outside the repository. It validates each utility's
module-local stable `PROGRAM_VERSION`, then builds and installs each utility in
deterministic name order under `${CARGO_HOME:-$HOME/.cargo}/bin`. It leaves no
repository-local `target/` directory.

Pass space-separated utility names to validate, release-build, and install only
those binaries:

```bash
./build.sh tree
./build.sh dos2unix
./build.sh tree dos2unix
./build.sh repoctl
```

A scoped build still formats the package and validates the shared library used
by every utility. It limits binary checks, CLI integration tests, release
artifacts, and installation to the selected names. Scoped installation uses
Cargo's `--no-track --force` mode so installed but unselected `rkit` binaries
remain untouched and Cargo's tracked package metadata remains unchanged. An
explicitly selected utility can overwrite a same-named destination binary.
Run `./build.sh` without targets for handoff and release validation.

## Tree

```text
tree v1.4.0
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

## dos2unix

```text
dos2unix v1.4.0
Preview or convert CRLF line endings — https://github.com/queone/rkit
Usage
  dos2unix [options] [--] FILE

  Preview FILE and display each CRLF pair as visible \r\n text.
  Use -- before a FILE whose name begins with a dash.

Options
  -f, --force    Convert CRLF pairs to LF in place
  -v, --version  Print version and exit
  -h, -?, --help Show this help message and exit
  --             End option parsing
```

Preview mode reads the complete regular file before printing it, replaces each
CRLF pair with visible `\r\n` text, and leaves every other byte unchanged. The
marker uses ANSI blue and help uses the same white command name as `tree` only
when standard output is a compatible terminal. Set `NO_COLOR`, use
`TERM=dumb`, or redirect output to receive plain bytes without ANSI escapes.

Use `-f` or `--force` before or after the file operand to replace each CRLF
pair with LF in place. Conversion preserves lone CR bytes, arbitrary non-UTF-8
bytes, the existing file inode, hard-link visibility, symbolic-link operands,
and Unix mode bits where supported. It accepts only regular files or symbolic
links to regular files.

Conversion reads the complete file before truncating and rewriting the same
open file. Open, inspection, and initial-read failures leave the file
unchanged. A write or flush failure after truncation can leave it partially
written; restore it from a backup or source control before retrying.

Examples:

```bash
dos2unix file.txt
dos2unix -f file.txt
dos2unix file.txt --force
dos2unix -- -file.txt
```

The Rust port preserves the Go utility's CRLF preview and byte-conversion
semantics. It uses the utility's independent stable version, terminal-aware
color, help flags, options on either side of the operand, and exit code 2 for
invalid arguments. The repository Cargo version is maintained separately.

## brew-update

```text
brew-update v1.3.5
Update, upgrade, and clean up Homebrew packages.
```

On macOS, `brew-update` requires Homebrew's `brew` executable to be installed
and available on `PATH`. It runs these operations in order: `brew update`,
`brew upgrade`, one combined upgrade for all installed casks, and
`brew cleanup -s`. Casks are read from `brew list --cask`; blank lines and
surrounding whitespace are ignored.

The exact terminal arguments `-v` and `--version` print the version without
running Homebrew. All other arguments are accepted for compatibility and do
not change the workflow. If Homebrew is missing or a command fails, the
diagnostic identifies the operation and advises verifying Homebrew and `PATH`
before retrying.

## repoctl

```text
repoctl 0.2.0
Control a collection of local Git repositories.
```

`repoctl` discovers immediate, non-hidden subdirectories containing a `.git`
directory. Its short commands and long aliases are:

```text
repoctl s, status [REPO ...]        Show status
repoctl p, pull [REPO ...]          Pull repositories
repoctl c, clone OWNER [REPO ...]  Clone all or selected owner repositories
repoctl b, build [REPO ...]         Run ./build.sh in repositories
```

Every command first flushes one unheaded processing row per applicable
repository. Rows contain Repo, Origin, and the selected operation, sorted by
Origin. Git and build output streams live as indented detail, followed by one
indented final status. The complete processing and final-status lines are
yellow in a compatible terminal; redirected output is plain.
The aligned Repo and Origin columns use four literal separator spaces.
`repoctl` resolves Origins, sorts the work, and processes repositories
sequentially. It completes each repository before starting the next.

`status` reports `👍 <branch>` for a clean tree and `❌ <branch>` for a dirty
tree. `pull` reports `Remote unavailable`, `Pulled`, `Already up to date`, or
`Pull failed`; `clone` reports `Cloned`, `Skipped`, or `Clone failed`; and
`build` runs an executable `./build.sh` from each repository root and reports
`Built`, `No build.sh`, or `Build failed`. A requested repository subset must
name discovered local repositories. A failed per-repository operation leaves
the remaining repositories running and makes `repoctl` exit non-zero.
Routine pull output that only repeats `Already up to date` is suppressed.

`clone` uses `gh repo list OWNER --json name --jq .[].name` when no repository
names are supplied and clones from `https://github.com/OWNER/REPO.git`. It
requires both Git and GitHub CLI for owner-wide listing; explicit repository
names require Git only.

## Governance

This repo is governed by an explicit session-entry contract for AI coding agents — see [`governa/operator-contract-rationale.md`](governa/operator-contract-rationale.md) for the design reasoning and [`AGENTS.md`](AGENTS.md) for the operational rules.
