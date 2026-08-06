# AC38 Investigate redundant dependency recompilation in build.sh

## Summary

Investigation (infra, code-only if a safe fix is found). A single `./build.sh` run visibly recompiles the same dependency graph three times — once each for `cargo clippy --all-targets --all-features` (`build.sh:612-618`), `cargo test --all-targets --all-features` (`build.sh:624-629`), and `cargo build --release` (`build.sh:635-639`) — despite all three sharing one `--target-dir`. This AC investigates whether that recompilation is reducible without weakening what each step verifies, and either lands a fix or records why it cannot be done.

## In Scope

### Investigation

- Determine, for each of the three `cargo` invocations above (and their `_run_scoped_build`-path equivalents at `build.sh:697-728`), which Cargo unit-graph identity differs (profile, feature set, `--target-dir` join, `RUSTFLAGS`, or target selection) and confirm via `cargo build --target-dir <dir> -Z unstable-options --unit-graph` or comparable evidence which recompiles are inherent to Cargo's per-profile/per-feature-set artifact isolation versus incidental (e.g. differing `--all-features` vs plain flags, differing `--verbose` paths, environment-variable drift between steps).
- Evaluate concrete mitigations if any incidental divergence is found — e.g. `sccache`/`cargo build --timings`-driven profile alignment, sharing one invocation via `cargo hack`, unifying `dev` and `test` profile settings in `Cargo.toml`, or reordering steps so the release build reuses `test`-profile artifacts where Cargo's caching rules allow it.
- Record findings directly in this AC's `## Migration findings`-style notes section (added during Refine/Implement) rather than a separate document.

### Files to modify (conditional — only if a safe fix is identified)

- `build.sh` — adjust cargo invocation flags/ordering only if doing so does not reduce clippy/test/release coverage.
- `Cargo.toml` — profile section changes only if required and only if release-artifact identity (the binaries installed via `cargo install`) is unaffected.

## Out Of Scope

- Any change that causes `cargo clippy` or `cargo test` to run with fewer features/targets than `--all-targets --all-features` today — that would weaken existing verification, not just speed it up.
- Any change to the release binaries' build flags or optimization profile.
- Introducing a new build-acceleration dependency (e.g. `sccache`, `cargo-nextest`) unless explicitly approved by the Director as a separate follow-on decision; this AC may recommend one but not add it silently.
- Fixing this repo's tool-availability gaps noted separately (missing `sd`/`sqlite-utils`/`ast-grep`/`pup` on this machine) — unrelated to compile-time behavior.

## Acceptance Tests

**AT1** [Manual] [Pre-release gate] — This AC's findings section states, for each of the three (or six, counting scoped-build variants) cargo invocations, whether its recompilation relative to the prior step is inherent to Cargo's profile/feature isolation or incidental, with supporting evidence (unit-graph diff, `--timings` output, or equivalent).

**AT2** [Automated] [Pre-release gate] — If a fix is applied to `build.sh` and/or `Cargo.toml`, `./build.sh` still runs `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features`, and `cargo build --release` to completion with unchanged pass/fail semantics, and total wall-clock time for a clean run is recorded before and after the change.

**AT3** [Manual] [Pre-release gate] — If no safe fix is found, the findings section states this explicitly with the reason (e.g. "inherent to Cargo profile isolation; no action taken") and no `build.sh`/`Cargo.toml` change is made.

## Status

`PENDING` — awaiting user authorization to begin implementation.
