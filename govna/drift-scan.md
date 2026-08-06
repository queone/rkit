# Drift Scan

`govna drift-scan` compares an adopted repo's governance artifacts against what `govna render-canon` would produce for it now, and emits a `govna/ac<N>-drift-scan-<canon-version>.md` stub listing the divergences for the Director to resolve.

Run it from the consumer repo root (no positional arguments) after `govna render-canon` or `govna apply`.

## Usage

```
govna drift-scan [flags]
```

Flags:

- `-f, --flavor code|doc` — overlay flavor (default: auto-detect from repo signals).
- `-s, --stack <name>` — CODE stack (default: inferred from manifests; not accepted with `--flavor doc`).
- `-j, --json` — also print a JSON report to stdout alongside the markdown emission.
- `-l, --diff-lines <N>` — diff truncation limit (default: 200).
- `-n, --repo-name <name>` — override repo name (default: basename of the target directory).
- `-h, --help` — show this help.

Preconditions: the target must be a govna-adopted repo (`AGENTS.md` present, plus a govna adoption signal — one of `govna/ac-template.md`, `govna/release.md`, `govna/build-release.md`, or a `CHANGELOG.md` row referencing `govna apply` or `govna render-canon`) and a git worktree (`.git/` present, `git` on `PATH`) — drift-scan needs git history to distinguish clean canon divergence from a locally-edited file.

## Classification

Each canon-governed file gets exactly one of 8 classifications, decided by an ordered check:

1. **Missing from target, preserve marker found** → `match` (suppressed) — a Director has already declared the omission intentional.
2. **Missing from target, no marker** → `missing-in-target`.
3. **Byte-equal to canon** → `match`.
4. **Mixed-content file, canon zone byte-equal** (see Mixed-content boundary registry below) → `match` — the repo-owned tail below the boundary is not compared.
5. **Listed in the expected-divergence registry** → `expected-divergence`.
6. **Otherwise divergent, preserve marker found** → `preserve`.
7. **Otherwise divergent, no marker, prior commits touching the file** → `ambiguity` — the file has a history of intentional local edits; a Director must decide sync vs. keep.
8. **Otherwise divergent, no marker, no prior commits** → `clear-sync` — safe to adopt canon's version.

`govna/metadata.txt` gets a ninth outcome layered on top: if the file is absent under a govna-adopted repo, it's forced to `migration-required` regardless of the byte-comparison result (see Migration-required items).

Files with no canon counterpart in the target's own flavor route to `target-has-no-canon` (see Cross-flavor orphan detection) rather than through this ordered check.

## Format-defining files

`govna/ac-template.md` and `AGENTS.md` are format-defining: any non-`match`, non-`expected-divergence` classification for these two files forces a sync entry in the emitted stub regardless of what the ordered check above produced (an `ambiguity` or `preserve` result still surfaces as a forced-sync note, since these two files define the shape every other AC and canon doc depends on).

## Expected-divergence registry

`plan.md` and `arch.md` are registered as expected per-repo divergence — canon ships them as content stubs, and every adopting repo is expected to carry repo-specific content in their place. Divergence here never routes to review.

## Mixed-content boundary registry

Files with a documented canon-above/local-below boundary, compared only above the boundary line for `match`:

| File | Boundary |
|---|---|
| `AGENTS.md` | `## Project Rules` |
| `govna/development-guidelines.md` | `## Project Practices` |
| `govna/editing-guidelines.md` | `## Project Practices` |

## Preserve-marker phrase set

A Director locks a local variant against future sync by placing one of these four phrases (with `<path>` replaced by the file's repo-relative path) in `CHANGELOG.md`'s `| Unreleased | |` row Summary column, or in any governance doc drift-scan scans:

- `preserve <path>`
- `do not sync <path>`
- `intentional divergence: <path>`
- `<path>: keep local`

A marker on a missing file suppresses `missing-in-target` to a suppressed `match`; a marker on a divergent file routes it to `preserve` instead of `ambiguity`/`clear-sync`.

## Cross-flavor orphan detection

Files present under the target's `govna/` tree with no canon counterpart in the target's own flavor classify as `target-has-no-canon`. This also catches files name-referenced (by path, in backticks or quotes) from an already-divergent target file but themselves absent from both the target's own flavor canon and the other flavor's canon — surfacing orphaned governance docs that drifted out of the shipped canon set entirely.

## Migration-required items

`govna/metadata.txt` absent from an otherwise govna-adopted target classifies as `migration-required` — the repo needs an explicit one-time write of the metadata file (via `govna render-canon`/`apply`) rather than a mechanical content sync, since there's no prior version to diff against.

## Canon-coherence precondition

Before comparing anything against the target, drift-scan checks that govna's own rendered canon is internally coherent — a registry-driven, canon-only precondition (`coherence_rules()`) that would catch cases like an overlay template drifting out of sync with its authority doc. The registry ships empty today (the mechanism exists; no rule has been added yet). If a future rule fails, drift-scan skips the target comparison entirely and emits a coherence-failure report instead, since a target scan is only meaningful when the canon it's compared against is itself sound.

## Emitted AC stub

drift-scan writes exactly one file, `govna/ac<N>-drift-scan-<canon-version>.md` (`N` allocated per the monotonic AC-numbering rule), conforming to `govna/ac-template.md`. Its `## In Scope` groups every non-`match` file into one of four buckets:

- **Sync** — `clear-sync`, `missing-in-target`, and any format-defining file forced to sync.
- **Migration** — `migration-required` items, under `## Migration findings`.
- **Out of scope** — `preserve` and `expected-divergence` items, explicitly excluded from this cycle's sync.
- **Review** — `ambiguity` and `target-has-no-canon` items, needing a Director routing decision before either syncing or preserving.

The stub carries an edit-detection marker (SHA-256 body hash). Re-running drift-scan against an unedited stub for the same canon version reuses the same AC number; running it against an edited stub fails with an error directing the Director to commit and delete the stub (to regenerate) or rename it off the `drift-scan-<version>` slug (to keep it as a standalone AC).

Pass `--json` to also print a machine-readable report (`header`: invocation, canon SHA, target, flavor and its source, repo name, govna/code-stack versions from metadata; `files`: one entry per scanned file with its classification, diff, prior commits, matched preserve markers, canon reference, and mixed-content boundary where applicable; `emitted`: the stub's path) alongside the markdown emission.
