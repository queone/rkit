# AGENTS.md

## Governed Sections

Edit only AGENTS.md; CLAUDE.md is a symlink that mirrors it.

Detail and rationale live in `governa/development-guidelines.md`, `governa/build-release.md`, `governa/development-cycle.md`.

Sections (fixed set):

- `Governed Sections`
- `Instruction Style`
- `Interaction Mode`
- `Approval Boundaries`
- `File-Change Discipline`
- `Review Style`
- `Base Rules`
- `Project Rules`

Rules:

- Preserve each section's semantic intent across edits.
- Add new rules under the best-fit existing section; the `##` section list is fixed by the Governed Sections contract.
- Edit sections in place; change section order or the `##` section list only when the user explicitly requests a contract amendment.
- Name the exact sections to change and keep edits local during every update.
- Edit this file as a governed config artifact, with rule-shaped bullets only.
- Use `##` for top-level sections and `###` for thematic groupings inside a section; cap header nesting at `###`.
- Apply the `## Instruction Style` section below to every new or rewritten instruction in this file.
- Prefer instruction wording that is easiest for an LLM to follow, while staying simple for a human operator.
- Treat AGENTS.md as the authoritative source for the rules it describes; conform overlay templates and other canon files to it (drift-scan's canon-coherence precondition catches violations).

## Instruction Style

- Apply these rules whenever an instruction is added or rewritten in AGENTS.md or any governance doc.
- Start each instruction with an action verb in imperative voice.
- Keep each instruction to one short, direct command.
- Carry scope or trigger conditions as the first imperative bullet of the section.
- Keep section headings clean — no parentheticals, no preamble prose between heading and bullets.
- Move other rationale or context to a separate note below the bullets.
- Split multi-action instructions into separate bullets.

Note: prefer wording that is easiest for an LLM to follow, while staying simple for a human operator.

## Interaction Mode

- Open each response with the answer, finding, open question, or one-sentence note on what you're about to do.
- Use terse flat bullets.
- Skip preambles, recaps, and implication walk-throughs.
- Create files and make repository edits only after explicit user authorization — including draft files, scratch scripts, scaffolding, and config tweaks.
- Make the smallest change that satisfies the request once authorized.
- Surface assumptions, ambiguities, and missing context before any direction-changing action.
- Operate as the Operator on every interaction (per `governa/roles.md`); the role is fixed and unannounced.
- Place each structured deliverable (AC, plan, doc draft, scope card) in its target file; never paste the full body in chat.
- Report each written deliverable with a one-paragraph chat summary plus the file path.
- Quote at most short, targeted snippets from a written file when discussing a specific change.

### Session Entry

- Treat AGENTS.md as the active operating contract for this repository.
- State "Governa contract loaded." before the first substantive Governa-governed action of a session, and only after internalizing AGENTS.md.
- Treat planning, editing, reviewing, command choice, and implementation work as substantive actions.
- Before any file change, confirm the gate set: AC status, explicit authorization, scoped edits, tests in the same pass, and no agent-run commits.
- Resolve instruction conflicts in this order: user instruction within authorized scope, then AGENTS.md, then referenced Governa docs, then model defaults.
- Stop and ask when a request bypasses a required Governa gate or lacks required authorization, scope, or context.

## Approval Boundaries

### General Gates

- Treat each authorization as scope-limited; require fresh approval for any new action, even when similar to a prior approved one.
- Require explicit approval for: create, delete, rename, publish, release, or any destructive change.
- Require explicit approval for: governance files, CI/release config, secrets handling, external integrations.
- Edit only the files listed in the AC's `## In Scope` section, even after the user has authorized implementation.
- Apply the drift-scan effective-scope exception in `### Drift-Scan Adoption` when a Director resolves an `ambiguity` item as `sync`.
- Stop and ask when a request is ambiguous, or when the change is hard to reverse.
- Wait for explicit user request before preparing, executing, publishing, deploying, or distributing — including drafting commit messages, commit commands, version bumps, or release notes.
- **Leave every `git commit` for the user to execute. No EXCEPTION.**
- Treat an explicit standalone `Package`, `package`, `pack`, or `prep` request in an active Ratified AC context as the trigger for release-prep bookkeeping (CHANGELOG row insertion, release-tag drafting, commit-command drafting, release-command presentation).
- Follow the Pre-Release Checklist in `governa/build-release.md` when executing release-prep bookkeeping.

### AC-First Workflow

- Treat every non-trivial change as AC-first work.
- Draft `governa/ac<N>-<slug>.md` before implementation using `governa/ac-template.md`; define scope, out-of-scope, and acceptance tests.
- Wait for explicit user confirmation that the AC is implementation-ready before starting implementation.

### Four-Phase Workflow

- Start each governed AC cycle in Scan when the AC is ready for adversarial review.
- Challenge the AC, repository behavior, referenced documentation, scope, edge cases, omissions, and testability during Scan.
- Keep Scan non-mutating; do not edit the AC or repository during Scan.
- Pause after Scan and await explicit Director instruction to Shape.
- Resolve Scan findings and incorporate settled Director decisions during Shape.
- Pause Shape when a Director-specific decision remains unresolved.
- Edit the AC during Shape; do not begin implementation during Shape.
- Pause after Shape and await explicit Director implementation-ready confirmation to Forge.
- Implement only the settled AC scope during Forge.
- Return to Shape when Forge reveals a contract, scope, or Director decision change.
- Return to Forge when Forge reveals an implementation-only correction.
- Include tests, adversarial verification, and defect correction in Forge.
- Run one exhaustive, non-mutating closure audit after Forge implementation, validation, adversarial verification, and defect correction.
- Keep the closure-audit working record in the active agent's session.
- Do not create a separate closure-audit artifact.
- Map every in-scope command entry point, provider/API fetch, normalized-table write, durable snapshot, stale fallback, freshness gate, and complete-snapshot reconciliation path in the closure audit.
- Check every in-scope governance instruction against `## Instruction Style` during the closure audit.
- Map every referenced governance document across applicable source, template, and rendered-consumer paths in the closure audit.
- Compare every discovered path with the active AC `## In Scope`, `## Out Of Scope`, and `## Acceptance Tests` sections.
- Record `Not applicable` with repository evidence when a path category is absent.
- Record every acceptance-test disposition and residual risk in the closure audit.
- Block Forge completion when any required implementation path is unmapped or unverified or any implementation finding remains open.
- Record pending Director review for manual acceptance tests without treating that pending review as an implementation finding or a Forge-blocking path gap.
- Return to Forge for implementation defects found by the closure audit.
- Return to Shape for scope, contract, or Director decision changes found by the closure audit.
- Report every acceptance-test disposition in the Forge completion report.
- Report every residual risk in the Forge completion report.
- State zero unresolved implementation findings in the Forge completion report before Ratify.
- Pause after Forge and await Ratify.
- Treat Ratify as the Director's final review of the delivered AC.
- Return Ratify feedback to Shape for contract or scope changes.
- Return Ratify feedback to Forge for implementation-only corrections.
- Keep Ratify complete only after the Director accepts the delivered work.
- Treat `Package` as the separate post-Ratify name for release preparation, not as a fifth AC phase.
- Start `Package` only after an explicit Director request; do not infer it from Ratify acceptance.
- Treat standalone `Package`, `package`, `pack`, and `prep` as equivalent names for `Package` only after Ratify acceptance.
- Preserve the existing release-prep implementation, behavior, commands, ordering, and approval boundaries during `Package`.

### Phase-Advancement Rules

- Treat only explicit Director phase-advancement language as authorization to enter the named next phase.
- Start an AC cycle only when the Director identifies the active AC and explicitly requests Scan.
- Apply an unnumbered phase instruction to the sole AC under `governa/`; require the AC number when multiple ACs are present.
- Treat a compound request as authorization for only the named phase.
- Pause before entering any phase not explicitly named by the Director.
- Treat ambiguous, unrelated, or implicit replies as non-advancing feedback.
- Interpret Scan, Shape, Forge, and Ratify as workflow phases only in the context of the active AC cycle.
- Interpret `Package` as the post-Ratify release-preparation action only after Ratify acceptance and an explicit Director request.
- Interpret standalone `Package`, `package`, `pack`, and `prep` as equivalent Package instructions only in that context.
- Do not interpret `run ./build.sh prep ...`, `pack the binary`, `prepare the build`, or non-standalone `prep` as workflow advancement.
- Treat ordinary coding language such as `build`, `package the binary`, or a package-manager command as unrelated to phase advancement.
- Require explicit operational wording such as `run ./build.sh` before executing a repository command; never infer a shell command from a phase name.

### Primary And Ancillary Scope

- Capture the resolved current working directory as the primary repository at session entry.
- Keep the primary repository and current phase visible in every phase report.
- Label work in another repository or path as `Ancillary work` only after the Director explicitly requests it.
- Report the ancillary repository or path and authorization separately from the primary phase.
- Prevent ancillary work from satisfying primary-repository scope, tests, validation, or phase gates.
- Restate the primary repository and paused phase when returning from ancillary work.

### AC Critique Gate

- Wait for the Director's explicit implementation-ready confirmation; the Director flags scope concerns in chat during this window.

### Pre-Implementation Verification

- Run this checklist after the Director resolves all review questions.
- Confirm each settled decision landed verbatim in the AC.
- Confirm ATs match settled wording.
- Confirm every new or rewritten instruction in AGENTS.md follows Instruction Style.
- List ✓ for each check and flag any gaps; authorize implementation only when clean.

### Drift-Scan Adoption

- Apply these rules whenever implementing a drift-scan-emitted AC.
- Treat the named target as effective implementation scope when the Director resolves an `ambiguity` item as `sync`, even when it is absent from `## In Scope`.
- Apply the resolved sync to the target file while leaving the emitted AC stub unchanged.
- Render canon into a scratch directory using `governa render-canon <scratch>`.
- Inspect changes per `## In Scope` item by running `diff -ru <scratch>/<path> <path>`.
- Record preserve decisions in the `| Unreleased | |` row's Summary column of `CHANGELOG.md` before re-running `governa drift-scan`.
- Use one of the marker phrases from `governa/drift-scan.md` `## Preserve-marker phrase set` for each preserve decision.
- Echo the preserve marker verbatim into the release message when the marker plus AC reference and summary fits the 80-character limit.
- Leave the preserve marker in the `| Unreleased | |` row when the combined length exceeds 80 characters.
- Ensure the parent directory exists for each `## In Scope` item: `mkdir -p "$(dirname <path>)"`.
- Categorize each `## In Scope` item as pure-canon or mixed-content before applying.
- Apply pure-canon items by copying from canon: `cp <scratch>/<path> <path>`.
- Apply mixed-content items by hunk-merge.
- Replace canon-zone content above the boundary heading (`## Project Rules` for AGENTS.md; `## Project Practices` for `governa/development-guidelines.md` and `governa/editing-guidelines.md`).
- Preserve the boundary heading and every line below it as repo-owned content.
- Re-run `governa drift-scan` after the sync without editing the emitted stub; confirm each synced file no longer appears in the new emission's `## In Scope` list.
- Run the repo's canonical validation (`./build.sh` or equivalent) before declaring the adoption complete.

Note: preserve markers in the `| Unreleased | |` row persist across releases (release prep does not modify that row); markers echoed into release-message rows are also recognized by future drift-scan runs from any CHANGELOG row. The AC stub is not a durable home because drift-scan's edit-detection guard blocks the required re-run after stub edits.

Note: mixed-content files (AGENTS.md, `governa/development-guidelines.md`, `governa/editing-guidelines.md`, and any other file with a documented repo-owned tail section) follow the hunk-merge convention in `governa/canon-cycle.md` `## Consumer-side workflow`.

## File-Change Discipline

- Prefer targeted edits over broad rewrites.
- Preserve user changes and unrelated local modifications.
- Update only the files required for the task plus directly affected docs, all in the same commit.
- Update affected docs in the same pass when a change adds a file, command, flag, or major decision.
- Complete every mid-implementation decision change in one pass — files, docs, and tests together; never leave a half-migrated state.
- Update user-facing docs when commands, setup, workflows, outputs, published structure, or operating instructions change.
- Update architecture, planning, or style docs only when materially affected.
- End every AC doc with a `## Status` section using one of `PENDING`, `IN PROGRESS`, or `DEFERRED` (with reason); use per-phase status for partial completion.
- Delete completed AC files at release prep per the development cycle — never mark `## Status` as `DONE`.
- Record follow-on improvements in `plan.md` (or note them to the user if no planning artifact exists); keep the current task strictly within its authorized scope.
- Use repo-relative paths or placeholders like `<project-root>` in committed content; before committing, scan staged content for `/Users/`, `/home/`, or `C:\` and replace any matches.
- **Include tests in the same pass as every code change — formatting, CLI output, and "small" changes alike.**
- **Record every correction about repo behavior as an edit to the governance doc that owns the topic; never as a memory entry, `feedback.md`, or session note.**

## Review Style

- Lead each review with findings and cite file paths and concrete behavior; skip preamble summaries.
- Prioritize bugs, regressions, missing tests, and drift from documented behavior.
- Treat AC-document ceremony issues as nits after implementation starts and the AC is expected to be deleted at release prep; prioritize defects that affect the delivered contract, implementation scope, tests, or release safety.
- Report "no issues" directly when none are found; note any residual risk or verification gaps.
- Keep completions terse — what changed, flat bullets, and a final `Awaiting <specific Director-initiated next>.` line; skip "What's in it" / "Main conclusion" / "Next steps" headers unless asked.
- Never prescribe commit, push, or release actions in Ratify; the Director triggers those — Ratify names what's pending, not what to do.
- Skip settled repo mechanics in completions, including symlink behavior, mirror mechanics, governance structure, and contract conventions.
- Default to plain text and simple bullets; reach for tables or richer structure only when content clearly benefits.
- Note skipped checks only when the omission is unusual or affects confidence.
- Run required validation gates, but report successful routine gates only when they materially affect confidence; always report failures and skipped required gates.
- Present architectural decisions to the director as: a recommendation when one viable option exists; two bounded options plus a recommendation when two exist; the best two plus a one-line note on the rest when more than two exist.
- Include the three-part self-review structure (Verified / Red-teamed / Not checked) defined in `governa/roles.md` in every substantial completion report, even when the default is terse.
- Start every Package completion report with the plain, unbulleted, unindented line `Package complete.`.
- Insert exactly one blank line after `Package complete.` before `Verified:`.
- Keep `Verified:`, `Red-teamed:`, `Not checked:`, and `Run below to release:` in the Package completion report; state `No commit or release command executed.` and present the exact drafted release command.

## Base Rules

### Build Verification

- Start a validation cycle when an authorized change pass is ready for validation.
- Run `./build.sh` as the first validation command in every validation cycle.
- Use `./build.sh` for repository-wide formatting validation, testing, vetting, linting, static analysis, and compilation checks.
- Do not invoke direct formatter, test, vet, lint, static-analysis, or repository-wide compilation commands before the first canonical build.
- Run prerequisite implementation commands such as code generation, dependency maintenance, and migrations before validation as needed.
- Use read-only inspection commands before validation when they do not claim repository health.
- Use isolated binary smoke commands before validation only when they do not claim repository health.
- Use a direct validation tool only to diagnose or correct a corresponding failure reported by the latest `./build.sh`.
- Scope each direct diagnostic or corrective command to the reported failure.
- Rerun `./build.sh` after any diagnostic or corrective command that changes files.
- Rerun `./build.sh` before running an unrelated direct validation command.
- Complete the validation cycle only after the final `./build.sh` succeeds.
- Treat work as unverified until the final `./build.sh` succeeds.
- Build smoke-test binaries with an explicit output path outside the repository.

### Versioning and Dependencies

- Follow semver: PATCH for invisible changes (fixes, refactors, tooling), MINOR for user-visible changes (commands, flags, schema, behavior); batch PATCH-level changes.
- Pin dependencies to explicit versions; document any reason to stay on an older version.

### Errors

- Wrap user-facing errors with operation context and recovery guidance.

### AC Mechanics

- Label each acceptance test with source axis (`[Automated]` / `[Manual]`) and timing axis (`[Pre-release gate]` default; `[Post-release verification]` explicit). See `governa/ac-template.md`.
- Name test identifiers, output labels, comments, and errors by behavior.
- Reserve AC, AT, Class, Part, and Round numbers for CHANGELOG rows, commit messages, and `Historical:` comments.
- Use the `Historical:` prefix on a comment only when it references a shipped AC and the context aids the reader; delete the reference if no longer relevant.

### Code Style and Conventions

- Pair every new CLI flag with a one-letter short form (standard, leads help output) and a long-form alias; migrate existing flags when their code is next touched.
- Follow existing repo patterns unless an approved improvement says otherwise.
- Comment public functions.
- Avoid product or vendor names in identifiers.
- Use product or vendor names only when an identifier names a real product-specific artifact or compatibility surface.

Note: `CLAUDE.md` is an example of an exempt identifier — it names the Claude Code-readable symlink that mirrors AGENTS.md.

### Tool Use

- Reach for `rg` (not `grep`/`ack`), `fd` (not `find`), `jq` (not `awk`/`python -c` on JSON), `sd` (not `sed -i`), `sqlite-utils` (not raw `sqlite3` cli), `ast-grep` (not regex on code), and `pup` (not regex on HTML).
- Send independent shell calls in a single message so they run in parallel.
- Reuse content from files already in conversation context; reach for `Read` only to fetch unseen content or check for recent changes.

## Project Rules

### Benchmarking

- Follow `benchmarking/rust-vs-go.md` whenever the Director explicitly requests a Go/Rust performance comparison, including “benchmark Go versus Rust,” “compare the Go and Rust ports,” or “rerun the Rust/Go performance comparison.”
- Benchmark `tree`, `dos2unix`, and `brew-update` by default.
- Honor an explicitly narrower Director scope.
- Preserve every existing dated Conclusion in `benchmarking/rust-vs-go.md`.
- Append a new dated Conclusion to `benchmarking/rust-vs-go.md` after each requested benchmark run.
- Record the hostname, timestamp, OS version, architecture, toolchains, source revisions, artifact identity, workloads, raw samples, derived results, and deviations in each Conclusion.
- Append an `UNAVAILABLE` Conclusion with the failed command, missing prerequisite, omitted dimension, and deviation when a requested measurement cannot complete.
- Avoid fabricating, silently skipping, or substituting a requested measurement.
- Treat routine execution of the existing benchmark protocol as exempt from a new AC.
- Draft a new AC when the benchmark methodology or scope changes.

- Keep repository package versions separate from individual utility program versions; derive release versions from repository package semver and change a utility's program version only when its AC explicitly requires it.

- Propagate every source-level change to `internal/` Go code (not governance text) to two sites in the same pass — the source file and its overlay template under `internal/templates/overlays/`; validate with `./build.sh`.
- Mirror every AGENTS.md change that applies to consumer repos across governa source `AGENTS.md`, `internal/templates/base/AGENTS.md`, and `internal/templates/overlays/doc/files/AGENTS.md.tmpl` (CODE consumer inherits base unchanged) before staging the commit.
- Mirror every change to a doc referenced from AGENTS.md across governa source `governa/`, `internal/templates/overlays/code/files/governa/`, and `internal/templates/overlays/doc/files/governa/` before staging the commit.
- Keep generated repos self-contained — every dependency lives in the generated repo itself.
- Track forward-looking work in `plan.md` only via IE entries (pre-rubric IEs or AC-pointers, per `plan.md`'s docstring).
- Limit `plan.md` structure to its existing top-level sections (`## Product Direction`, `## Ideas To Explore`); add new content only as IE bullets.
- Draft skeleton AC stubs first for every multi-AC track, then register one AC-pointer IE per stub.
- Remove each AC-pointer IE when its AC ships and the AC file is deleted at release prep.
- Skip the IE entirely (at Director's discretion) when an AC is drafted and implemented inside a single cycle.
