# AC35 Add interactive picker to web

## Summary

Add an interactive fuzzy-finder result picker to the `web` DuckDuckGo-search utility (delivered without one in `governa/ac30-web.md`), settling the `skim`-as-library vs. shell-to-`fzf` architecture question deferred there.

## In Scope

### Files to modify

- `src/web.rs` — add the interactive picker path (live filtering, keyboard navigation, a title/snippet/link preview pane matching Go's `go-fzf` usage), reached the same way Go reaches it: whenever `-j`/`--json` is not given.
- `Cargo.toml` and `Cargo.lock` — add the selected picker dependency (candidate: `skim` v5.6.1 as an embedded library) or none, if shelling out to an external `fzf` binary instead.
- `README.md` and `arch.md` — document the interactive mode and, if the shell-out path is chosen, the `fzf` runtime prerequisite.

## Out Of Scope

- Everything already delivered by AC30 (query building, scraping, JSON output, `--open N`, browser-opening, injectable HTTP transport and opener).
- Other interactive-picker utilities.
- Release preparation beyond this AC.

## Open Decisions (Refine)

- Select `skim`-as-library vs. shell-to-`fzf`. Tradeoffs already researched during AC30's Audit: `skim` matches Go's self-contained-binary architecture (no extra install for the end user) at higher Rust implementation risk; shelling to `fzf` matches this repo's existing shell-out precedent (`dl`→yt-dlp, `pman`→azm) at lower implementation risk, but adds an external runtime dependency Go's original didn't have — and `fzf` was confirmed not installed on the reference development machine.
- If `skim`: confirm its library API supports a `go-fzf`-style custom preview-pane callback before committing to it.
- If `fzf`: confirm the "not installed" error/recovery message, matching `dl.rs`'s yt-dlp-missing precedent.
- Confirm whether AC30's `--open N` stays available as a non-interactive alternative once the picker ships, or is superseded by it.

## Acceptance Tests

**AT1** [Automated] [Pre-release gate] — `./build.sh` validates the binary declaration and version.

**AT2** [Automated] [Pre-release gate] — Tests cover picker invocation via an injectable seam (no test drives a real interactive terminal session), single-selection-opens-browser behavior (matching Go's first-selection-only quirk even under multi-select), cancel/no-selection handling, and diagnostics.

**AT3** [Automated] [Pre-release gate] — Existing rkit utilities remain passing under package-wide validation.

## Status

`PENDING` — skeleton stub; awaiting Audit.
