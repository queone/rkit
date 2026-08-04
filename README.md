# rkit

`rkit` provides small, standalone Rust command-line utilities. It currently
includes `tree` for directory hierarchies, `dos2unix` for previewing or
converting CRLF line endings, `brew-update` for maintaining Homebrew packages
on macOS, `repoctl` for operating on collections of local Git repositories,
`certgen` for generating self-signed certificates and CSRs, and `certls` for
inspecting verified TLS certificates, plus `rn`, `rncap`, and `rnlower` for
bulk filename renaming and case conversion, and `vdrop`, `vjoin`, and `vkeep`
for ffmpeg-based video editing and joining, `bak` for dated file or directory
backups, `days` for calendar-day calculations, `decolor` for removing ANSI
SGR color sequences from files or streams, `dl` for downloading video with
yt-dlp, `pgen` for generating memorable passwords, and `pman` for calling
Azure REST APIs.

## Why

Use `rkit` when compact, cross-platform implementations of common command-line
tools are preferable. Certificate and TLS operations use the pinned vendored
OpenSSL dependency declared in `Cargo.toml`; vjoin uses its pinned JSON parser,
and the other utilities use the Rust standard library and shared package code.

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
declared-path stable `PROGRAM_VERSION`, then builds and installs each utility in
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

## bak

```text
bak v2.0.0
Usage: bak <file|directory>
```

`bak` copies one file or directory to a destination suffixed with the local
calendar date as `YYYYMMDD`. The local date comes from the host `date` command
and falls back to the UTC calendar date when that command is unavailable.
Existing destinations receive alphabetic suffixes (`a`, `b`, …, `z`, `aa`, …).
Files are created exclusively, directory trees retain their source modes where
supported, and failures report `backup failed:` without overwriting an
existing destination.

## days

```text
days v1.1.0
```

`days` accepts `YYYY-MM-DD` and case-insensitive `YYYY-MMM-DD` dates, signed
day offsets (`days -11`, `days +6`, or `days 6`), and two-date comparisons.
Date comparisons use UTC; offset calculations use the local calendar clock,
read from the host `date` command with a UTC fallback when it is unavailable.
Invalid operands return a contextual error and nonzero status. The long help
screen is available with `-h`, `-?`, or `--help`; `-v` and `--version` print
only the one-line version.

## decolor

```text
decolor v1.1.1
```

`decolor` removes CSI SGR sequences (`ESC[...m`) from a named file or piped
stdin while preserving all other bytes. With no input stream it prints usage;
`-h`, `-?`, and `--help` show the same help screen, and `-v`/`--version` are
terminal one-line version requests. File-read failures return status 1;
stdin-read diagnostics are non-fatal, matching the Go utility.

## dl

```text
dl v2.0.0
```

`dl` downloads a video with `yt-dlp` to `FILENAME URL`, appending `.mp4` when
the filename doesn't already end in it (case-insensitively), and refuses an
existing destination. `-u`/`--update` upgrades `yt-dlp` to its nightly build
and then prints the `dl` and `yt-dlp` versions. `-v`/`--version` and help are
handled before any `yt-dlp` presence check, so `dl -v` always prints exactly
one line even without `yt-dlp` installed; this deliberately diverges from the
Go original, which required `yt-dlp` for every invocation and printed a
second `yt-dlp <version>` line. Colored output routes through the shared
terminal-color detection instead of the Go original's hardcoded ANSI codes.

## pgen

```text
pgen v1.2.3
```

`pgen` prints three passwords: an underscore-joined diceware phrase (default
3 words, or 1–9 via a `NUMBER` operand), a dash-joined "strong memorable"
form with the first word title-cased and one independently drawn digit
(0–9) appended to an independently chosen word, and a 16-character
alphanumeric password starting with a capital consonant. Randomness comes
from the pinned OpenSSL CSPRNG with rejection sampling; this deliberately
diverges from the Go original, whose appended digit always equaled the
chosen word's index and whose alphanumeric selection had a modulo bias. The
word list is the embedded 7,776-word EFF large wordlist. An out-of-range or
non-numeric `NUMBER` prints `NUMBER must be 1 thru 9.` on stdout and exits 1;
extra operands are ignored and fall back to the 3-word default.

## pman

```text
pman v2.0.0
```

`pman` calls Microsoft Graph or Azure Resource Manager with a bearer token
obtained from the `azm` command-line utility, selecting `azm -tmg` or
`azm -taz` by URL substring. `-d`/`--data` sets a JSON request body; the
response body is printed to stdout as-is regardless of HTTP status, matching
the Go original. A token not starting with `eyJ` prints a stderr warning but
the request still proceeds. `-?`, `-h`, and `--help` print usage and exit 0;
this deliberately diverges from the Go original, which had no help flag. The
HTTP client is a minimal standard-library implementation using the pinned
OpenSSL dependency for TLS; no new dependency was added.

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
repoctl 0.3.0
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

`status` and `pull` print one completed, unheaded result row per applicable
repository. `build` and `clone` first flush a live processing row, stream
indented details, and finish with one indented final status. Rows contain Repo,
Origin, and the selected result or operation, sorted by Origin. Complete result,
processing, and final-status lines are yellow in a compatible terminal;
redirected output is plain.
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
Other status and pull diagnostics appear as uncolored indented details beneath
the completed result row.

When terminal color is enabled, `build` passes `GOVERNA_FORCE_TTY=1` to a
governed child build only when the variable is absent. This preserves the
child's normal colorized output while `repoctl` streams and indents it. An
inherited value is preserved, and disabled-color execution does not inject one.

`clone` uses `gh repo list OWNER --json name --jq .[].name` when no repository
names are supplied and clones from `https://github.com/OWNER/REPO.git`. It
requires both Git and GitHub CLI for owner-wide listing; explicit repository
names require Git only.

## rn

```text
rn v1.5.0
Bulk file re-namer
Usage: rn "OldString" "NewString" [-f]
```

`rn` scans the current directory in filename order, skips directories, and
replaces every occurrence of the old string. Without `-f` it prints a dry run;
with `-f` it performs native filesystem renames. A third operand other than
`-f` retains the historical dry-run behavior.

## rncap and rnlower

```text
rncap v2.0.0
rnlower v2.0.0
```

Both utilities ask for confirmation before renaming every current-directory
entry, including directories and symlinks. `rncap` title-cases Unicode
letter/digit words across punctuation; `rnlower` applies Unicode lowercase
conversion. Existing destinations are reported on standard error and skipped,
while other rename errors are reported and processing continues.

## certgen

```text
certgen v2.0.0
Usage: certgen <common-name>
```

`certgen` interactively creates a 2048-bit RSA private key, a certificate
signing request, and a ten-year self-signed certificate for the supplied common
name. It writes `<common-name>.key`, `.csr`, and `.crt` with the historical PEM
labels and Unix modes, and includes the common name as a DNS SAN. Answer `Y` to
the confirmation prompt; any other response aborts with status 1.

## certls

```text
certls v2.0.0
Usage: certls FQDN[:PORT]
```

`certls` connects with certificate verification enabled, using port 443 when
the port is omitted. On macOS it supplements OpenSSL's PEM trust paths with
the SystemRootCertificates keychain so trust matches the Go utility. It rejects
malformed multi-colon targets and reports the verified certificate's RFC3339
validity window and DNS SANs. It reports the negotiated protocol with the
`==> TLS version` label.

## vkeep and vdrop

```text
vkeep v0.3.0
vdrop v0.3.0
Usage: vkeep START [END] [-a] <input>
Usage: vdrop START [END] [-a] <input>
```

`vkeep` extracts a section and `vdrop` removes a section and joins the
remaining parts. Timestamps accept `MM:SS`, whole seconds, and `HH:MM:SS` for
sources longer than one hour; `END` may be omitted or set to `end`. Outputs are
named beside the input with `_` inserted before trailing digits, and existing
outputs are never overwritten. The default path stream-copies at keyframes;
`-a, --accurate` re-encodes for frame accuracy. `vdrop -x` or
`--crossfade[=SECS]` dissolves an interior join, defaulting to 0.5 seconds.

Both commands require `ffmpeg` and `ffprobe` on `PATH` and print an input/output
summary after a successful operation.

## vjoin

```text
vjoin v0.1.0
Usage: vjoin INPUT1 INPUT2
```

`vjoin` writes `merged.mp4` in the current directory without overwriting an
existing file. Each input must contain video and audio. Portrait inputs receive
a blurred background; horizontal and square inputs receive black padding.
Video is normalized to 1920x1080 at 30 fps, audio to 48000 Hz, and output uses
H.264 CRF 18, AAC 192k, and fast-start metadata. Rotation metadata is included
when classifying orientation. It requires `ffmpeg` and `ffprobe` on `PATH`.

## Governance

This repo is governed by an explicit session-entry contract for AI coding agents — see [`governa/operator-contract-rationale.md`](governa/operator-contract-rationale.md) for the design reasoning and [`AGENTS.md`](AGENTS.md) for the operational rules.
