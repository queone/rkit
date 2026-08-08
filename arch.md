# rkit Architecture

## Purpose

Provide small, standalone Rust command-line utilities with deterministic
behavior. Certificate and TLS operations use the pinned vendored OpenSSL
dependency through one shared verified platform-trust boundary, vjoin uses
pinned serde_json for ffprobe parsing, fr uses the
pinned regex crate for its search/replace matching, and the other utilities
avoid third-party runtime dependencies.

## System Summary

All production TLS client connectors route through `src/tls.rs`. Certls uses it
directly; pman supplies it to attune and sms; web and cash5 retain their local
timeout-aware TCP transports while sharing the connector and trust store.

The root Cargo package builds the `tree`, `dos2unix`, `brew-update`, `repoctl`,
`certgen`, `certls`, `rn`, `rncap`, `rnlower`, `vdrop`, `vjoin`, `vkeep`, `bak`,
`days`, `decolor`, `dl`, `pgen`, `pman`, `fr`, `sms`, `jy`, `mdview`,
`retotal`, `web`, `cash5`, and `swatch` binaries. Each binary
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
- `src/bin/certgen.rs`: `certgen` process streams and exit-code boundary
- `src/bin/certls.rs`: `certls` process streams and exit-code boundary
- `src/bin/rn.rs`: `rn` process streams and exit-code boundary
- `src/bin/rncap.rs`: `rncap` process streams and exit-code boundary
- `src/bin/rnlower.rs`: `rnlower` process streams and exit-code boundary
- `src/bin/bak.rs`: `bak` process streams and exit-code boundary
- `src/bin/days.rs`: `days` process streams and exit-code boundary
- `src/bin/decolor.rs`: `decolor` process streams and exit-code boundary
- `src/bin/dl.rs`: `dl` process streams and exit-code boundary
- `src/bin/pgen.rs`: `pgen` process streams and exit-code boundary
- `src/bin/pman.rs`: `pman` process streams and exit-code boundary
- `src/bin/fr.rs`: `fr` process streams and exit-code boundary
- `src/bin/sms.rs`: `sms` process streams and exit-code boundary
- `src/bin/jy.rs`: `jy` process streams and exit-code boundary
- `src/bin/mdview.rs`: `mdview` process streams and exit-code boundary
- `src/bin/retotal.rs`: `retotal` process streams and exit-code boundary
- `src/bin/web.rs`: `web` process streams and exit-code boundary
- `src/bin/cash5.rs`: `cash5` process streams and exit-code boundary
- `src/bin/swatch.rs`: `swatch` process streams and exit-code boundary
- `src/tree.rs`: tree parsing, traversal, path handling, and rendering
- `src/dos2unix.rs`: CRLF parsing, preview, conversion, and diagnostics
- `src/repoctl.rs`: repository discovery, operation execution, sorting, rendering, and diagnostics
- `src/certgen.rs`: RSA key, CSR, certificate generation, artifact writing, and prompts
- `src/attune/`: provider-neutral specification loading and Azure reconciliation through pman's verified HTTPS transport
- `examples/attune/`: synthetic six-kind project for offline validation and public usage guidance
- `src/certls.rs`: verified TLS connection, certificate extraction, and reporting
- `src/tls.rs`: shared peer-verifying OpenSSL connector and platform trust loading
- `src/rn.rs`: byte-aware filename replacement, dry runs, and native renames
- `src/rncap.rs`: interactive Unicode title-casing and entry renames
- `src/rnlower.rs`: interactive Unicode lowercase conversion and entry renames
- `src/video_edit.rs`: shared vkeep/vdrop parsing, media probing, edit planning, execution, summaries, and diagnostics
- `src/vjoin.rs`: ffprobe JSON parsing, rotation-aware orientation, normalized filter construction, and join execution
- `src/bak.rs`: dated destination selection, recursive copying, mode preservation, and backup diagnostics
- `src/days.rs`: Gregorian date parsing, offset/comparison calculations, formatting, and diagnostics
- `src/decolor.rs`: byte-preserving CSI SGR filtering, file/stdin routing, and diagnostics
- `src/dl.rs`: yt-dlp presence checks, extension normalization, download/upgrade execution, and diagnostics
- `src/pgen.rs`: embedded-wordlist selection, rejection-sampled randomness, and password formatting
- `src/pman.rs`: injectable token-source and HTTP-transport traits, a minimal HTTP/1.1-over-TLS client, and request/response handling
- `src/fr.rs`: sorted hidden-directory-skipping tree walk, `file`-gated text detection, regex search/replace/highlight, and atomic file rewriting
- `src/sms.rs`: XDG config resolution, legacy-path migration, INI parsing, and form-encoded HTTP POST via the reused `pman` transport
- `src/jy.rs`: JSON/YAML format detection, bidirectional conversion, token-aware colorizing, and ANSI-stripped input handling
- `src/mdview.rs`: GFM rendering via `comrak`, `<details>`/`<summary>` disclosure preprocessing, HTML document assembly, and injectable file/browser output
- `src/github-markdown.css`, `src/github-markdown-css-LICENSE`: embedded, SHA-256-pinned upstream stylesheet and its MIT license
- `src/retotal.rs`: CSV/aligned parsing, two-stage numeric formatting, signature-gated consolidate/re-tally dispatch
- `src/web.rs`: DuckDuckGo query building, a timeout-aware HTTPS transport, CSS-selector HTML scraping, an injectable interactive result picker (`nucleo-picker`-backed) with a TTY-gated numbered-list fallback, JSON output, and browser-opening (reusing `mdview`'s `BrowserOpener`)
- `src/cash5/`: NJ Cash 5 draw fetching (primary API plus `scraper`-based `lottonumbers.com` backup), XDG state persistence with legacy-era pruning, a collision-avoiding recommendation engine, statistics, match analysis, an odds table, iTerm2 "winning circle" image rendering, and CLI dispatch — one submodule per Go source-file concern (`api`, `dates`, `display`, `match_analysis`, `model`, `recommend`, `render`, `stats`, `store`, `strategy`)
- `src/swatch.rs`: argument parsing, rkit-owned xterm ramp tables, palette/grid/background rendering, contrast selection, and injectable color-mode coverage
- `src/gomono.ttf`, `src/gomono-LICENSE`: embedded, SHA-256-pinned "Go Mono" TrueType font (extracted from `golang.org/x/image/font/gofont/gomono`) and its BSD-3-Clause license
- `src/color.rs`: shared terminal color policy
- `src/lib.rs`: narrow package-binary library boundary
- `tests/tree_cli.rs`: compiled `tree` behavior coverage
- `tests/dos2unix_cli.rs`: compiled `dos2unix` behavior and file-effect coverage
- `tests/repoctl_cli.rs`: `repoctl` command, operation, sorting, and output coverage
- `tests/certgen_cli.rs`: compiled `certgen` artifact and prompt coverage
- `tests/certls_cli.rs`: compiled `certls` parsing, verification, and output coverage
- `tests/rn_cli.rs`: compiled `rn` parsing, dry-run, collision, and rename coverage
- `tests/rncap_cli.rs`: compiled `rncap` prompt, Unicode, and rename coverage
- `tests/rnlower_cli.rs`: compiled `rnlower` prompt, Unicode, and rename coverage
- `tests/vdrop_cli.rs`: compiled `vdrop` CLI and edit behavior coverage
- `tests/vjoin_cli.rs`: compiled `vjoin` probe, filter, and join coverage
- `tests/vkeep_cli.rs`: compiled `vkeep` CLI and edit behavior coverage
- `tests/bak_cli.rs`: compiled `bak` backup, collision, and argument coverage
- `tests/days_cli.rs`: compiled `days` date, offset, error, and version coverage
- `tests/decolor_cli.rs`: compiled `decolor` file, pipe, ANSI, and version coverage
- `tests/dl_cli.rs`: compiled `dl` stub-yt-dlp, extension, and version coverage
- `tests/pgen_cli.rs`: compiled `pgen` structural password-property coverage
- `tests/pman_cli.rs`: compiled `pman` stub-azm, injectable-seam, and local-TCP-transport coverage
- `tests/fr_cli.rs`: compiled `fr` walk, search/replace, and version coverage
- `tests/sms_cli.rs`: compiled `sms` config-migration, skeleton-config, and local-TCP-transport coverage
- `tests/jy_cli.rs`: compiled `jy` conversion, colorizing, piped/file input, and version coverage
- `tests/mdview_cli.rs`: compiled `mdview` rendering, output-flag, diagnostics, and version coverage
- `tests/retotal_cli.rs`: compiled `retotal` consolidate/re-tally, signature, and version coverage
- `tests/web_cli.rs`: compiled `web` argument-validation, diagnostics, and version coverage
- `tests/cash5_cli.rs`: compiled `cash5` argument-validation, diagnostics, and version coverage
- `tests/swatch_cli.rs`: compiled `swatch` shorthand, alias, rendering, diagnostics, and documentation coverage
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
- `src/certgen.rs`: reusable `certgen` behavior
- `src/certls.rs`: reusable `certls` behavior
- `src/color.rs`: shared terminal color detection and rendering
- `tests/tree_cli.rs`: end-to-end `tree` tests
- `tests/dos2unix_cli.rs`: end-to-end `dos2unix` tests
- `tests/repoctl_cli.rs`: end-to-end `repoctl` tests
- `tests/build_cli.sh`: Bash 3.2-compatible build and release routing tests
- `govna/development-cycle.md`: workflow from roadmap through release
- `govna/ac-template.md`: acceptance-criteria template for new work
- `govna/build-release.md`: build, test, and release rules

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
2. Resolve each Origin before operation execution and sort the work by Origin (the untrimmed value, matching Go's behavior — sorting is unaffected by the display trim below).
3. Complete status and pull operations before rendering their colored Repo,
   Origin, and result rows, followed only by retained uncolored details. The displayed Origin has a trailing `.git` suffix stripped; the `origin` field itself stays untouched everywhere it's used functionally, including `clone`'s actual `git clone` argument.
4. Render and flush build and clone processing rows before starting work.
5. Stream build and clone stdout and stderr as indented details, then render one
   colored indented final status.
6. Inject `GOVERNA_FORCE_TTY=1` only for a color-enabled governed build when
   the parent environment does not define it; preserve inherited values.
7. Process repositories sequentially, retaining the aggregate failure state.
8. Return the aggregate operation exit code after all selected repositories
   have been attempted.

### certgen

1. Parse one common name or a terminal version request.
2. Build the fixed compatibility subject and prompt for confirmation.
3. Generate a 2048-bit RSA key, CSR, and ten-year self-signed certificate with
   the common name as a DNS SAN and server-authentication extensions.
4. Write the three PEM artifacts with the historical names, labels, and modes,
   then list matching files and return the operation status.

### certls

1. Parse one `FQDN[:PORT]` target and default the port to 443.
2. Create a verified OpenSSL TLS connector from platform trust paths, adding
   macOS SystemRootCertificates keychain roots when available.
3. Connect with the host as the verification name and require a peer
   certificate.
4. Report the negotiated TLS protocol with the `TLS version` label, its
   RFC3339 validity window, and DNS SANs.

### rn

1. Parse the old string, optional replacement, and optional `-f` switch while
   treating every other third operand as dry-run mode.
2. Read and sort current-directory entries, skipping directories.
3. Replace all occurrences in each matching filename and either print the
   aligned dry-run row or perform the native rename.
4. Continue per-file failures, return status 1 only when no filename matched,
   and preserve filename bytes on Unix where available.

### rncap and rnlower

1. Flush the confirmation prompt and accept only `Y` or `y` after trimming.
2. Read and sort every current-directory entry, including directories and
   symlinks.
3. Apply Unicode title-casing or lowercase conversion, skip existing
   destinations, and continue reporting rename failures.
4. Return the historical status and output streams after the complete scan.

### vkeep and vdrop

1. Validate ffmpeg/ffprobe, input media, timestamps, ranges, and derived output names.
2. Select whole-file copy, fast keyframe copy, accurate re-encode, hard-cut concat, or interior crossfade plans.
3. Execute the plan without overwriting existing outputs and remove temporary concat files on every exit path.
4. Probe the resulting output and render the aligned input/output summary.

### vjoin

1. Reject an existing `merged.mp4` before probing inputs.
2. Parse each ffprobe JSON result, require video and audio, and classify orientation from dimensions and rotation metadata.
3. Build the orientation-specific normalization graph and concatenate both inputs in order.
4. Execute ffmpeg and report `Created: merged.mp4` only after success.

### bak

1. Validate one source operand and resolve the local `YYYYMMDD` suffix via the host `date` command with a UTC fallback.
2. Select the first exclusive destination in the alphabetic collision sequence.
3. Copy a file or recursively copy a directory while preserving source modes.
4. Report contextual backup failures without overwriting an existing destination.

### days

1. Parse numeric and case-insensitive named-month Gregorian dates.
2. Apply local-calendar offsets (host `date` with a UTC fallback) or calculate UTC and absolute date differences.
3. Render the historical year-plus-days breakdown and contextual input errors.

### decolor

1. Select a named file, piped stdin, or terminal help according to the argument and TTY state.
2. Remove only CSI SGR sequences while preserving all other input bytes.
3. Stream output and report file or stdin diagnostics with the documented status.

### dl

1. Handle `-v`/`--version` and help before any `yt-dlp` presence check; this deliberately diverges from Go, which gated every invocation on that check.
2. Check `yt-dlp` presence before a download or `-u`/`--update` upgrade.
3. Normalize the destination extension to `.mp4` and refuse an existing destination.
4. Run `yt-dlp` with inherited stdio and report contextual success or failure.

### pgen

1. Parse an optional 1–9 `NUMBER` operand, defaulting to 3 and rejecting out-of-range or non-numeric values with the exact bounds message.
2. Select unique words from the embedded EFF large wordlist via OpenSSL-CSPRNG rejection sampling.
3. Render the diceware phrase, a title-cased dash-joined strong-memorable form with one independently drawn digit, and a 16-character alphanumeric password.

### pman

1. Handle `-v`/`--version` and help before the `azm` presence check, matching Go's order; add a help flag that Go lacked.
2. Route token acquisition to `azm -tmg` or `azm -taz` by URL substring through an injectable `TokenSource`.
3. Warn on stderr for a token not prefixed `eyJ` without blocking the request.
4. Send the request through an injectable `HttpTransport`, printing the raw response body regardless of HTTP status.
5. Construct production HTTPS connectors through `src/tls.rs`, preserving peer and hostname verification with OpenSSL paths and macOS Keychain roots.
6. Keep reconciliation ordering, prune gates, scope comparison, and tag-merge behavior covered by injected-provider tests; keep the public six-kind example offline-validatable without live identifiers.

### fr

1. Dispatch on argument count: single-argument search, `FROM TO` show-only (highlighting `FROM` only; `TO` is accepted but unused, matching Go), or `FROM TO -f`/`--force` replace-and-write.
2. Walk the tree depth-first in sorted order from the current directory, skipping hidden directories entirely and non-regular entries (symlinks included).
3. Gate each remaining regular file through the host `file -b --mime-type` command, accepting `text/*`, `application/xml`, and `application/json`.
4. Match with a pinned `regex` pattern compiled once; an invalid pattern silently matches nothing for the whole run.
5. Highlight matching lines in place, or replace matches via a temp file plus rename that preserves the original file's mode.
6. Abort the whole run with a contextual diagnostic on any read/write I/O failure during the walk, matching Go's fatal-on-`walkFn`-error behavior.

### sms

1. Dispatch on argument count and flags; a bad count or an unrecognized single argument prints usage and exits 0, matching Go.
2. Reject `-v`/`--version` combined with another operand with a diagnostic; this deliberately diverges from Go, which silently treated the combined `-v` as the phone number.
3. Resolve `~/.config/sms/config.ini` (honoring an absolute `XDG_CONFIG_HOME`), migrating a legacy `~/.smsrc` on first use: skip a symlink with a warning, warn and prefer the new path when both exist, and fall back to copy-plus-delete across a filesystem boundary.
4. Parse the `[global]` `svcurl`/`svckey` pair from the resolved file with a hand-rolled INI reader; an unreadable or malformed file degrades to the same "not defined" diagnostics as Go's discarded `ini.LoadFile` error.
5. Reuse pman's HTTP transport and the shared verified TLS trust boundary.
6. POST the form-encoded `key`/`message`/`phone` fields through the reused `pman` `HttpTransport`; print `Error. HTTP error code = <status>` for a non-200 response and a contextual diagnostic for a transport failure.

### jy

1. Strip ANSI SGR sequences from file or piped input before parsing, reusing `decolor`'s filter.
2. Detect JSON first (JSON is a YAML subset); fall back to YAML. A bare `null`/empty document counts as neither and reports `Not JSON nor YAML`.
3. Convert the detected value to the other format with 2-space indent via a hand-written `Yaml`/`serde_json::Value` bridge (`yaml-rust2` has no serde integration).
4. Print plainly for `-d`, or colorize by walking `yaml-rust2`'s token stream and coloring the source span between consecutive token markers; `-c` colorizes a file's own raw content without converting it.
5. Comments are not tokenized by the scanner and pass through uncolored, diverging from the Go original's comment-preserving lexer.

### mdview

1. Read the resolved input file as UTF-8 text (a stricter requirement than the Go original, which processes arbitrary bytes).
2. Preprocess `<details>`/`<summary>` regions ahead of Markdown rendering: recursively isolate them, protect fenced/indented code, inline code spans, HTML comments, and raw containers from being misread as disclosure tags, and drop malformed or orphaned disclosure tags. An unclosed `<details>` fully unwraps, demoting its own `<summary>` to plain text.
3. Render each region via `comrak` with GFM extensions enabled and `render.unsafe` left at its default `false`, so raw HTML becomes the literal comment `<!-- raw HTML omitted -->` and dangerous links are neutralized; `<details>`/`<summary>` tags always emit with every attribute discarded.
4. Assemble the HTML document, escaping only the filename-derived title; embed the base URL, the SHA-256-pinned stylesheet, and the rendered body as trusted.
5. Write persistent output (`-o`, mode `0o644`, refuses an existing destination) or a uniquely named temporary file (mode `0o600`) opened through an injectable `BrowserOpener` — left on disk on success, removed on a failed open.

### retotal

1. Dispatch on the file's first non-empty line: the retotal-output header (`DESCRIPTION`/`MO/AVG`/`YR/AVG`, 2+-space separated) selects re-tally; anything else selects consolidate.
2. Consolidate detects CSV vs. space-aligned input (strip quoted substrings, check for a remaining comma, else a 2+-space run), parses rows (CSV headers case-insensitive with an explicit TYPE column; aligned headers case-sensitive with TYPE inferred from a `" - "` split), drops total/duplicate-header rows, and computes MO/YR totals.
3. Re-tally requires the exact trailing signature line first — refusing and leaving the file untouched otherwise — then drops the old TOTAL row and any prior signature before recomputing.
4. Numeric formatting is two-stage: force 2 decimal places, then add thousand separators only when `abs(value) >= 1000` (otherwise passed through unchanged).
5. Write a signed `<stem>.txt` for consolidate (new file, mode `0o644`) or rewrite the input in place for re-tally (existing file, mode preserved).

### web

1. Reject an empty/whitespace-only query before building any request.
2. Build the alphabetically-sorted, form-urlencoded DuckDuckGo query URL and send it through a timeout-aware HTTPS transport (a local, injectable variant of `pman`'s pattern, since `pman.rs`'s own transport has no configurable timeout and is outside this utility's file scope).
3. Scrape `.result`/`.result__title a`/`.result__url`/`.result__snippet` via `scraper`, decoding each result's DuckDuckGo redirect-wrapper link.
4. `-j` prints the results as JSON and returns; `--open N` opens the Nth result's link directly. Otherwise, when stdout and stdin are both terminals, open an injectable interactive picker (`nucleo-picker`-backed; a single `<title>  <truncated snippet>` line per row, since the crate has no split-preview-pane mechanism) — Enter opens the selected link, Esc/cancel is a silent no-op, matching Go's `go-fzf`-backed picker exactly including the cancel behavior. When not a terminal, fall back to a numbered list instead (`web`'s AC30 default). Every open path routes through an injectable `BrowserOpener` (the default system opener, or a `-b`/`--browser`-specified command).
5. Apply the documented timeout/User-Agent/Referrer defaults whenever the corresponding flag and environment variable are both absent — deliberately diverging from Go, whose real behavior silently sent no timeout and empty headers in that case despite documenting the same defaults.
6. Preserve its timeout-aware TCP transport while constructing the client connector through the shared verified TLS trust boundary.

### cash5

1. Load cached draws from `$XDG_STATE_HOME/cash5/draws.json` (migrating a legacy `$XDG_CONFIG_HOME/cash5/draws.json` on first use) and prune pre-2014-09-14 (1-40-pool-era) rows, rewriting the file atomically only when something was pruned.
2. On the default (no-flag) path, check connectivity via an injectable seam, then page through the primary NJ Lottery API for missing recent draws (year-long windows, 5x retry with backoff on a transient 500) — falling back to a `scraper`-based `lottonumbers.com` HTML scrape only on a primary 404.
3. Display the last 10 draws, the current jackpot (live fetch with a cached-estimate fallback), the last winning numbers with any repeat history, and the closest prior 3+-number matches — all dates rendered in Eastern Time via a hand-rolled US-DST rule (valid for the entire post-2014-09-14 data range), not the operator's OS-local timezone.
4. Generate 5 recommendations (most/least common by position, most frequent overall, hot last-30-days, consecutive-pair avoidance), each guaranteed absent from the full historical-winners set via a deterministic swap/lexicographic-search fallback chain.
5. Preserve its timeout-aware primary and backup transports while constructing client connectors through the shared verified TLS trust boundary.
6. `-s`/`--stats`, `-m [N]`/`--match-analysis`, and `-o [N]` render statistics (chi-squared uniformity, birthday-paradox duplicates), match/pattern analysis, and an odds/EV table respectively; `-o`/`-m`'s optional-value parsing runs before general flag dispatch, matching Go's cobra-can't-do-optional-values pre-parse hack. `-v`/`--version` prints only the version, diverging from Go's full-usage-screen `-v`, matching the `mdview`/`retotal` fix.
7. In an iTerm2 session (an injectable `TerminalCapability` seam, not a bare env read, so `run_daily`'s and `display_match_analysis`'s existing stdout-content tests stay deterministic), render the "winning circle" — numbers 1-45 around a ring, winners spoked and highlighted — via hand-plotted pixels on a raw RGBA buffer (`ab_glyph` for glyph rasterization, `png` for encoding, no canvas-drawing crate), then emit it as an iTerm2 inline image. One emission in the daily summary (last winning numbers); one per displayed draw in match analysis (governed by `-m N` exactly, no independent cap, matching Go).

### swatch

1. Parse primary subcommands `p, palette`, `g, grid`, and `b, backgrounds` through one shorthand-and-alias command boundary.
2. Render the standard 16 colors, 216-color cube, grayscale range, and eleven rkit-owned 11-step ramps without a third-party dependency.
3. Render foreground grids, reverse-mode background grids, and background-ramp rows through the shared terminal color policy.
4. Select xterm black 16 or bright white 15 for automatic background contrast, or validate and apply a caller-selected foreground index.
5. Preserve labels, borders, indices, tokens, and layout when color is disabled.
6. Inject color mode internally for deterministic rendering tests while keeping process streams at the binary entrypoint.

## AC Lifecycle Control Flow

The governed change path is `Draft → Audit → Refine → Implement → Ratify → Package`. Draft creates the AC; Audit, Refine, Implement, and Ratify are the four AC phases; Package is post-Ratify release preparation and is not a fifth phase.

## Architecture Notes

- Match each utility's functional behavior to its Go source — not the specific libraries, library names, or standard-library coverage Go happened to use for it; a Go stdlib package implies neither a required nor a forbidden Rust dependency. Add a dependency only when it is necessary to reach that functional behavior, and only via an approved AC. Currently approved: pinned `serde_json = 1.0.145` for vjoin ffprobe parsing, and pinned vendored `openssl = 0.10.73` (with `openssl-src = 300.6.1+3.6.3`) for certificate and TLS operations. Use the Rust standard library everywhere else.
- Implement `bak`, `days`, and `decolor` filesystem, Gregorian-date, terminal, and CSI-SGR behavior with the Rust standard library and internal helpers; do not add a runtime dependency for these utilities.
- Implement `pgen` and `pman` external-command, randomness, and HTTP behavior with the Rust standard library plus the already-pinned `openssl` crate (CSPRNG for `pgen`, TLS for `pman`'s hand-rolled HTTP/1.1 client); do not add a runtime dependency for these utilities.
- Implement `fr`'s regex search/replace with the pinned `regex` crate — the repo's first pattern-matching dependency, added because no reasonable standard-library substitute exists for RE2-derived matching. Implement `sms`'s HTTP POST and INI parsing with the Rust standard library plus the already-pinned `pman` `HttpTransport`/`TcpHttpTransport`; add no HTTP or INI dependency for `sms`.
- Implement `jy`'s YAML parsing, serialization, and token-level scanning with the pinned `yaml-rust2` crate (chosen over `saphyr` specifically because its `scanner` module exposes token type and position data that `saphyr-parser`'s event-only API does not); reuse the already-pinned `serde_json` for the JSON side, adding no second JSON dependency.
- Implement `mdview`'s GFM rendering with the pinned `comrak` crate, chosen specifically because its default-safe rendering (`render.unsafe` left `false`, `tagfilter` extension enabled) reproduces the Go original's `goldmark` default sanitization byte-for-byte on the `<!-- raw HTML omitted -->` placeholder; reuse the already-pinned `openssl` crate's SHA-256 for the embedded-stylesheet checksum, adding no hashing dependency.
- Implement `retotal`'s CSV parsing by hand (no dependency; the tested feature surface — quoted fields containing commas, no multi-line fields — is bounded); reuse the already-pinned `regex` crate for its two small patterns, adding no new dependency.
- Implement `web`'s HTML scraping with the pinned `scraper` crate — the direct Rust analog of Go's `goquery`, no simpler stdlib-only substitute exists for parsing a live third-party site's markup. Implement `web`'s interactive result picker with the pinned `nucleo-picker` crate, chosen over `skim` after comparing real dependency trees (52 resolved packages vs. 238, no `tokio`/`ratatui`) — the tradeoff is no built-in split preview pane, so each row renders as a single `<title>  <truncated snippet>` line instead. `--open N` remains available as a non-interactive alternative alongside the picker.
- Implement `cash5`'s backup HTML scraping by reusing the already-pinned `scraper` crate (no new dependency) and its primary/backup HTTPS transport by reusing the already-pinned `openssl` crate's macOS-keychain trust-store fix first proven necessary for `web`. Implement `cash5`'s iTerm2 "winning circle" image rendering with the pinned `ab_glyph` (pure-Rust glyph rasterization) and `png` (encoding, real DEFLATE compression) crates — no `image`/`imageproc` dependency, since pixel plotting is hand-rolled directly on a raw RGBA buffer, matching the Go original's own approach.
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
- Declare one literal `PROGRAM_VERSION` in each `src/bin/<utility>.rs` and pass
  it into the testable module.
- Add no third-party dependency without an approved AC.
