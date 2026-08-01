# Build and Release

## Build and Test Rules

- use one canonical local build command and keep this document current
- run formatting, static checks, tests, and packaging through that command or documented sequence
- do not trigger release work during routine implementation

This repo uses a self-contained `build.sh` for all build, release-prep, and release work. No external governa tools are required; everything runs directly from `build.sh`.

### Build Presentation

- Reuse the canonical build color policy and palette across supported CODE stacks.
- Color phase headings, command previews, status values, failures, prep output, and release output by semantic role.
- Emit plain output when stdout is not a terminal.
- Emit plain output when `NO_COLOR` is set.
- Emit plain output when `TERM=dumb`.
- Require a 256-color-capable terminal before emitting ANSI sequences.
- Preserve plain-text content and output streams when color is disabled.
- Keep self-contained build scripts compatible with Bash 3.2.

## Minimum Validation

- formatting passes
- static checks pass
- automated tests pass
- changed docs match actual behavior

## Canonical Build Commands

```bash
./build.sh
```

To scope the run to selected commands:

```bash
./build.sh <target> [<target> ...]
```

Use space-separated target names. Supported CODE stacks may retain package-wide shared-code validation while limiting target-specific checks, tests, artifacts, and installation to the selected targets.

Run `./build.sh` without targets for repository-wide validation. Release-prep pre-change and post-change validation always use this package-wide form.

## Independent Utility Versions

- Treat the repository/package version as the version input and release metadata governed by the existing release mechanism.
- Require one normalized record for each installable utility with its canonical target name, declaration location, declared version, and `--version` invocation.
- Accept only `^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$` as a strict stable SemVer declaration.
- Require `--version` to exit 0, print exactly `<utility-id> <MAJOR.MINOR.PATCH>` plus its newline to stdout, and write nothing to stderr.
- Validate every normalized record before compilation, installation, or release-metadata writes.
- Reject missing, empty, malformed, duplicate, orphaned, and mis-mapped records with a non-zero error that names the utility and recovery action.
- Preserve all independent utility declarations and outputs during repository release prep.

## Pre-Release Checklist (`Package`, `package`, `pack`, or `prep`)

Do not start this checklist unless the director explicitly requests standalone
`Package`, `package`, `pack`, or `prep` in the active Ratified AC context.
Do not treat `./build.sh prep ...` or ordinary build-preparation language as a
workflow request.

The operator flow is two steps:

1. **Run `./build.sh prep vX.Y.Z "message"`.** Stages version bumps, inserts the CHANGELOG row, deletes completed AC files, sweeps matching AC-pointer IE lines from `plan.md`, runs validation builds before and after, and prints the canonical release command. The agent determines the version (semver classification from the AC's scope) and drafts the release message (≤ 80 characters) before invoking prep. Flags: `--dry-run`/`-n` prints intended writes without touching the working tree; `--no-build`/`-B` skips the pre- and post-check builds.
2. **Run the printed release command (`./build.sh vX.Y.Z "message"`).** Shows `git status --short`, lists every git step it will execute, and prompts for interactive confirmation. On approval it orchestrates `git add → commit → tag → push tag → push branch`.

Present only the release command after prep; do not add trailing commentary about wrapper routing or prompts. The director already knows.

### Appendix: what prep does

`./build.sh prep` runs nine phases internally so the operator flow above stays short. Each phase has a clear failure mode:

1. **Validate inputs.** Semver pattern (`vX.Y.Z`), message non-empty and ≤ 80 characters.
2. **Validate git state.** Inside a git work tree, target tag does not exist yet, HEAD is not at the latest tag with a clean working tree.
3. **Pre-check build.** `./build.sh` runs before any writes; skip it with `--no-build`/`-B` only for single-utility repositories or with `--dry-run`/`-n`.
4. **Detect and validate version targets.** Scan `cmd/*/main.go` for `programVersion` and `internal/templates/version.go` (each presence-gated). Match both inline and grouped declarations. Bump one detected utility target in a single-utility repository. In a multi-utility repository, validate exactly one strict stable SemVer declaration and successful exact `--version` output for every utility, skip all individual utility bumps, and fail before any write when validation fails.
5. **Detect CHANGELOG targets + fail-fast idempotency guard.** Root `CHANGELOG.md` and `internal/templates/CHANGELOG.md` (template-repo case). If any target already contains a row for the target version, prep exits with a fatal error before any writes.
6. **Parse AC refs.** `AC[0-9]+` scan on the release message; composites like `AC60+AC61` yield multiple refs.
7. **Apply writes.** Version bumps (per-file idempotent no-op when the file already has the target value); CHANGELOG row insertion under `| Unreleased | |`; AC file deletions (AC files are deleted whole; there are no separate companion files); AC-pointer IE-line sweep from `plan.md` (lines matching `→ governa/ac<N>-` for each released AC). Skipped when `--dry-run`/`-n`. Idempotent re-runs leave already-swept lines alone.
8. **Post-check build.** `./build.sh` run after writes; skipped with `--no-build`/`-B` or `--dry-run`/`-n`.
9. **Print release command.** Labeled block: `release command:` followed by the indented command `./build.sh vX.Y.Z "message"`.

CHANGELOG row shape (enforced by prep's insertion code and by convention):

- File shape: `# Changelog` heading, then a 2-column markdown table (`| Version | Summary |` with a `|---------|---------|` separator); first data row is `| Unreleased | |`, followed by one row per release (e.g., `| <version> | <AC-ref>: <one-line summary> |`).
- During a drift-scan adoption cycle, the `| Unreleased | |` row's Summary column may carry preserve marker phrases (per `governa/drift-scan.md` `## Preserve-marker phrase set`). Release prep inserts the new release row beneath the Unreleased row without modifying it, so any markers there persist. When the marker phrase plus the AC reference and summary fits the 80-character release-message limit, echo the marker into the release message so it lands in the new release row for cleaner separation; when it does not fit, leave the marker in the Unreleased row, where it remains recognized by future drift-scan runs from any CHANGELOG row.
- Summaries are single-line, ≤ 500 characters; lead with the AC reference if any.
- Versions are unprefixed (`0.29.0`, not `v0.29.0`).
- Do not backfill historical tags or invent alternative shapes (Keep-a-Changelog, sectioned `## vX.Y.Z`, etc.).
