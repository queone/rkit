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
yt-dlp, `pgen` for generating memorable passwords, `pman` for calling
Azure REST APIs, `fr` for regex search and replace across a file tree, `sms`
for sending SMS messages via textbelt.com, `jy` for converting between JSON
and YAML, `mdview` for viewing GitHub Flavored Markdown in a browser or
writing it as HTML, `retotal` for consolidating and re-tallying financial
TOTALS, `web` for searching DuckDuckGo from the command line, and `cash5` for
NJ Cash 5 lottery data, statistics, and number recommendations, plus `swatch`
for inspecting xterm colors and rkit ramp tables.

## Why

Use `rkit` when compact, cross-platform implementations of common command-line
tools are preferable. Certificate and TLS operations use the pinned vendored
OpenSSL dependency declared in `Cargo.toml`; vjoin uses its pinned JSON parser;
`fr` uses the pinned `regex` crate for its search/replace matching; `jy` uses
the already-pinned `serde_json` plus the pinned `yaml-rust2` crate for YAML;
`mdview` uses the pinned `comrak` crate for GitHub-Flavored-Markdown
rendering; `web` and `cash5` use the pinned `scraper` crate for CSS-selector
HTML scraping; `web` additionally uses the pinned `nucleo-picker` crate for
its interactive result picker; `cash5` additionally uses the pinned
`ab_glyph` and `png` crates for its iTerm2 "winning circle" image
rendering; and the other utilities use the Rust standard library and
shared package code.

## swatch

```text
swatch v1.0.0
Xterm palette and rkit ramp inspector
```

`swatch` prints the complete xterm 256-color palette and eleven rkit-owned
11-step ramp tables. Its primary subcommands and full-word aliases are
`p, palette` for the complete palette, `g, grid` for a bordered foreground
grid, and `b, backgrounds` for background-ramp swatch rows. An omitted or
empty sample token defaults to `TOKEN`.

Use `g -r`/`g --reverse` to render ramp colors as cell backgrounds and add
`-f INDEX`/`--foreground INDEX` to select the cell text color. The background
view accepts the same foreground option and otherwise selects xterm black 16
or bright white 15 for contrast. Indices must be between 0 and 255.

Color appears only on a supported terminal when `NO_COLOR` is absent. Plain
output preserves every label, border, index, token, and layout. The utility
uses only the Rust standard library and existing rkit shared code.

```bash
swatch p
swatch palette
swatch g --reverse HEADER
swatch b --foreground 15 LABEL
```

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

## cash5

```text
cash5 v2.0.0
NJ Cash 5 daily numbers recommender
```

`cash5` fetches NJ Cash 5 draw history from the NJ Lottery API (falling back
to scraping `lottonumbers.com` on a primary 404), caches it at
`$XDG_STATE_HOME/cash5/draws.json` (migrating a legacy
`$XDG_CONFIG_HOME/cash5/draws.json` on first use and pruning pre-2014-09-14
draws from the 1-40-number-pool era), and by default displays the last 10
draws, the current jackpot, the last winning numbers with any repeat history,
the closest prior matches, and 5 collision-avoiding number recommendations —
none of which has ever won historically. `-s`/`--stats` shows frequency
tables, chi-squared uniformity tests, and a birthday-paradox repeat analysis;
`-m [N]`/`--match-analysis` shows per-draw closest-match and cross-dataset
pattern analysis; `-o [N]` shows an odds/EV table; `-a`/`--all` lists every
draw; `-f`/`--fetch-all` backfills full history; `-d DATE` dumps a draw's raw
fields. `-v`/`--version` prints only the version — this deliberately diverges
from the Go original, which showed the full usage screen instead, matching
the fix already applied to `mdview` and `retotal`. Draw dates always render
in US Eastern Time (the pool's home timezone) rather than the operator's
local timezone, avoiding a Go quirk where the same instant renders a
different calendar day depending on where the tool runs. In an iTerm2
session, the daily summary and each displayed match-analysis draw also emit
a "winning circle" diagram — numbers 1-45 arranged around a ring with
winners highlighted and spoked — rendered with the pinned `ab_glyph` and
`png` crates and the same embedded "Go Mono" font the Go original uses, sent
as an inline image via iTerm2's escape-sequence protocol.

## jy

```text
jy v2.0.0
JSON / YAML converter — https://github.com/queone/rkit
Usage
  jy [options] [file]
```

`jy` detects whether input (a file argument or piped stdin) is JSON or YAML
and prints it in the other format with 2-space indent: JSON in, YAML out;
YAML in, JSON out. A bare `null`/empty document counts as neither format.
Output is colorized by token — mapping keys and mapping-value strings blue,
other strings green (yellow immediately after an anchor/alias), plain
numbers/bools magenta, anchors/aliases yellow — unless `-d` prints plainly.
`-c` prints a file's own content colorized without converting it. Piped or
file input is ANSI-stripped before parsing, reusing `decolor`'s CSI-SGR
filter. YAML comments pass through uncolored: the underlying scanner
(`yaml-rust2`) does not tokenize them, unlike the Go original's lexer.

## web

```text
web v2.0.0
Usage
  web [options] [query]
```

`web` searches DuckDuckGo and, by default, opens an interactive fuzzy-finder
result picker (via the pinned `nucleo-picker` crate) whenever stdout and
stdin are both real terminals — type to filter, arrow keys to navigate,
Enter to open the selected result in the default browser, Esc/Ctrl-C to
cancel silently (matching Go's `go-fzf`-backed picker, including its
silent-no-op-on-cancel behavior). Each row shows `<title>  <snippet,
truncated to fit>` rather than Go's separate split preview pane —
`nucleo-picker` has no split-pane mechanism, a deliberate lighter-weight
tradeoff over embedding `skim` (see `governa/ac35-web-picker.md`). When
stdout or stdin isn't a terminal (piped, scripted, CI), `web` falls back to
a numbered list of `title`/`link` results instead — an interactive picker
can't run there regardless of crate choice. `-j`/`--json` prints the full
`title`/`link`/`snippet` array as JSON and skips the picker entirely;
`--open N` opens the Nth (1-based) listed result directly, also skipping
the picker (`-b`/`--browser` overrides the opener with a specific command
for either path). `-t`/`--timeout`, `-u`/`--user-agent`, and `-r`/`--referrer`
(or their `DUCKGO_TIMEOUT`/`DUCKGO_USER_AGENT`/`DUCKGO_REFERRER`
environment equivalents) default to 5 seconds, a realistic browser
User-Agent, and `https://google.com` respectively when unset — this
deliberately diverges from the Go original, whose real (undocumented)
behavior sent no timeout and empty headers in that case, despite
documenting the same defaults. An empty query is rejected before any
request is sent.

## retotal

```text
retotal v2.0.0
retotal FILE
retotal -h | --help
```

`retotal` reads CSV or space-aligned financial data (columns: TYPE,
DESCRIPTION, MO/AVG, YR/AVG, NOTES) and writes an aligned `<stem>.txt`
summary with a computed `TOTAL` row, signed with a recalculation note as
its last line. Running it again on that signed output file re-tallies in
place: it recomputes `TOTAL` from the current rows and rewrites the file,
refusing to run at all — and leaving the file untouched — if the signature
line is missing or altered. CSV headers are matched case-insensitively;
aligned-input headers are matched case-sensitively (a real asymmetry, not
a bug). Values under 1,000 are left as-is; larger values get thousand
separators. `-h`/`--help`, no arguments, or more than one argument all
print the usage screen and exit 0.

## mdview

```text
mdview v2.0.0
Usage
  mdview [-o FILE] FILE
```

`mdview` renders a local Markdown file as GitHub Flavored Markdown (tables,
strikethrough, task lists, autolinks) via the pinned `comrak` crate and
either opens it in the default browser or, with `-o`/`--output FILE`
(`-o=FILE`/`--output=FILE` also accepted), writes the HTML to that file
without opening a browser — refusing an existing destination either way.
`<details>`/`<summary>` are the only supported raw HTML disclosure elements;
their attributes are always discarded, and Markdown (including GFM tables)
keeps rendering inside them. Every other raw HTML element, disclosure markup
outside a `<details>` block, and dangerous links (e.g. `javascript:`) are
omitted or neutralized — `comrak`'s default-safe rendering replaces raw HTML
with the literal comment `<!-- raw HTML omitted -->`, matching the Go
original's `goldmark` default. Relative links and images resolve from the
resolved input's directory via a `file://` base URL. The embedded
`github-markdown-css` stylesheet is checked against a pinned SHA-256 at
compile time. Browser-opening is macOS (`open`) and Linux (`xdg-open`)
only; Windows is out of scope, matching `jy`.

## fr

```text
fr v2.0.0
Usage:
  fr <REGEX>                -> search-only mode
  fr <FROM> <TO>            -> show-only mode
  fr <FROM> <TO> -f         -> replace-and-write mode
```

`fr` walks the current directory tree, skipping hidden directories (names
starting with `.`) without descending into them, and considers every regular
file that the host `file -b --mime-type` command reports as `text/*`,
`application/xml`, or `application/json`. Single-argument mode highlights
regex matches; `FROM TO` show-only mode highlights `FROM` matches without
writing (`TO` is accepted but unused, matching the Go original); `FROM TO -f`
(or `--force`) replaces matches in place via a temp file plus rename,
preserving the original file's mode. An invalid regex silently matches
nothing across the whole run. Matches highlight in red and filenames in
yellow when color is enabled.

## sms

```text
sms v2.0.0
SMS CLI utility 2.0.0
sms <CellPhoneNum> <Message>
sms -v | --version
sms -y Create skeleton ~/.config/sms/config.ini file
Visit https://textbelt.com for more info.
```

`sms` sends a text message through the https://textbelt.com API using a
`svcurl`/`svckey` pair read from `~/.config/sms/config.ini` (`-y`/`--init`
creates a skeleton file). A legacy `~/.smsrc` is migrated into the new path
on first use, with a symlink skipped (not migrated) and a warning when both
paths exist. `-v`/`--version` combined with another operand is rejected with
a diagnostic; this deliberately diverges from the Go original, which
silently treated a combined `-v` as the phone number. A non-2xx response
prints `Error. HTTP error code = <status>` and exits 1. The HTTP client is
the same minimal standard-library implementation `pman` uses; no new network
dependency was added.

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

## attune

`attune` reconciles Azure Resource Manager and Microsoft Graph resources from
provider-neutral YAML specifications. It supports DNS record sets, security
groups, app registrations, role definitions, role assignments, and resource
groups without a local state file.

```text
attune validate              # validate specs offline
attune plan                  # read live state and show changes
attune apply                 # create, update, and permitted prune operations
```

Live commands require an authenticated Azure CLI session from `az login`.
Configuration is read from the nearest `attune.yaml`; precedence is flag,
environment, configuration file, then built-in default. DNS pruning defaults
to enabled. Identity, role, and resource-group pruning default to disabled and
must be enabled explicitly with their corresponding flags or configuration.
Role assignments accept `group` and its case-insensitive `securityGroup` alias,
`servicePrincipal`, or `user` as `principalType`. A literal directory object ID
may omit `principalType`; a named principal must provide it, and `attune
validate` enforces this rule offline.

Normal plans print resource keys and concise summaries, but omit DNS values,
tag values, memberships, owners, role actions, credentials, and provider
response bodies. `-d`/`--diagnostic` adds non-secret account and target
grounding. Attune writes no local state, cache, telemetry, copied specs, or
diagnostic artifacts; serviced-repository data is sent only to the configured
Azure provider endpoints during an operator-requested live command.

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
repoctl c, clone NAME               Clone NAME (scoped; see below)
repoctl c, clone OWNER REPO ...     Clone selected owner repositories
repoctl c, clone OWNER/REPO         Clone one repository directly
repoctl l, list                     List repositories in scope
repoctl b, build [REPO ...]         Run ./build.sh in repositories
```

`status` and `pull` print one completed, unheaded result row per applicable
repository. `build` and `clone` first flush a live processing row, stream
indented details, and finish with one indented final status. Rows contain Repo,
Origin, and the selected result or operation, sorted by Origin. Complete result,
processing, and final-status lines are yellow in a compatible terminal;
redirected output is plain.
The aligned Repo and Origin columns use four literal separator spaces. A
trailing `.git` suffix is trimmed from the displayed Origin (noise most of
the time); the untrimmed URL is still what `clone` actually clones from.
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

`clone NAME` (a single operand with no `/`) is scoped: if `NAME`
case-insensitively matches the authenticated GitHub user (`gh api user --jq
.login`) or one of their orgs (`gh api user/orgs --jq '.[].login'`), it
bulk-clones every repository under that owner via `gh repo list NAME --json
name --jq .[].name`, exactly like today. Otherwise it searches those same
owners for a repository named exactly `NAME` and clones it — zero matches is
an error, and a name found under more than one scoped owner is also an
error asking for `OWNER/REPO` instead. `clone OWNER REPO ...` and `clone
OWNER/REPO` always clone directly from `https://github.com/OWNER/REPO.git`
with no scope check, for cloning any repository regardless of ownership.
`list` prints every `owner/repo` in scope, one per line, without cloning.
Scoped forms (`clone NAME`, `list`) require both Git and GitHub CLI;
`OWNER REPO ...` and `OWNER/REPO` require Git only.

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

This repo is governed by an explicit session-entry contract for AI coding agents — see [`govna/operator-contract-rationale.md`](govna/operator-contract-rationale.md) for the design reasoning and [`AGENTS.md`](AGENTS.md) for the operational rules.
