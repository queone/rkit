# rkit Architecture

## Purpose

Provide small, standalone Rust command-line utilities with deterministic
behavior. Certificate and TLS operations use the pinned vendored OpenSSL
dependency, vjoin uses pinned serde_json for ffprobe parsing, and the other
utilities avoid third-party runtime dependencies.

## System Summary

The root Cargo package builds the `tree`, `dos2unix`, `brew-update`, `repoctl`,
`certgen`, `certls`, `rn`, `rncap`, `rnlower`, `vdrop`, `vjoin`, `vkeep`, `bak`,
`days`, `decolor`, `dl`, `pgen`, and `pman` binaries. Each binary
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
- `src/tree.rs`: tree parsing, traversal, path handling, and rendering
- `src/dos2unix.rs`: CRLF parsing, preview, conversion, and diagnostics
- `src/repoctl.rs`: repository discovery, operation execution, sorting, rendering, and diagnostics
- `src/certgen.rs`: RSA key, CSR, certificate generation, artifact writing, and prompts
- `src/certls.rs`: verified TLS connection, certificate extraction, and reporting
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

## Architecture Notes

- Use the Rust standard library for general behavior, pinned `serde_json = 1.0.145` for vjoin ffprobe parsing, and pinned vendored `openssl = 0.10.73` (with `openssl-src = 300.6.1+3.6.3`) for certificate and TLS operations; do not add another dependency without an approved AC.
- Implement AC24 filesystem, Gregorian-date, terminal, and CSI-SGR behavior with the Rust standard library and internal helpers; do not add a runtime dependency for these utilities.
- Implement AC25 external-command, randomness, and HTTP behavior with the Rust standard library plus the already-pinned `openssl` crate (CSPRNG for `pgen`, TLS for `pman`'s hand-rolled HTTP/1.1 client); do not add a runtime dependency for these utilities.
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
