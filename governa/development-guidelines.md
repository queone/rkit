# Development Guidelines

Engineering guidance for any agent or contributor working in this repo.
These are durable coding practices, not workflow or process rules.
For workflow, see `development-cycle.md`. For validation, see `build-release.md`.
Sections above ## Project Practices are governa-maintained canon and update via canon syncs; repo-specific practices in ## Project Practices.

## Identifier Strategy

- Choose a primary key strategy early and document it in `arch.md`
- Prefer surrogate keys for internal identity; keep external IDs as indexed attributes
- When integrating multiple external ID systems, maintain an explicit mapping layer rather than assuming IDs are interchangeable

## Schema And Data Migrations

- Treat schema changes as first-class events: version them, document them, test the migration path
- Never assume old data fits new schemas — write migration logic or fail explicitly
- When a migration changes identity or key structure, audit all foreign key references in the same change

## External Integration Patterns

- Validate external data at the boundary; do not trust upstream shape or completeness
- When reconciling data from multiple sources, define a clear precedence order and document it
- Cache external data locally with explicit TTL or versioning; never silently serve stale data as fresh

## Generated Artifact Propagation

- When source-of-truth code is duplicated into templates or rendered examples, fixes must propagate to all copies in the same change
- Grep the full repo for the pattern being changed before considering a fix complete
- If a template and its rendered output diverge, the template is authoritative
- Keep `build.sh` self-contained; do not add sourced production helper modules.

## Error Handling And Validation

- Validate at system boundaries (user input, external APIs, file I/O); trust internal code
- Fail explicitly rather than silently degrading — a clear error is better than wrong output
- Static analysis and linting errors are build failures, not warnings

## Testing Expectations

- Tests are part of implementation, not a follow-up step
- Every new function and error path should have a test before the work is presented as complete
- If a code path cannot be tested without mocking infrastructure that is out of scope, document the coverage gap explicitly rather than silently skipping it
- Label tests that require live systems or manual verification as `[Manual]`

## Dependency And Import Hygiene

- Prefer standard library over external dependencies when the capability is equivalent
- When adding a dependency, justify it — convenience alone is not sufficient
- Keep import paths consistent after renames or reorganizations

## CLI Usage Formatting

- All commands must accept `-h`, `-?`, and `--help` as help flags
- Help output uses a shared formatting function for consistent layout
- "Usage:" is rendered in bold white
- Each flag line is indented 2 spaces; descriptions align at column 38
- Short and long flag forms are combined on one line (e.g. `-v, --verbose`)
- When adding new flags, add the entry to the shared usage formatter — do not rely on framework defaults

## Documentation Alignment

- Docs ship with the code change that introduces the behavior
- If a doc references a function, flag, or file path, verify it still exists before publishing
- Architecture docs (`arch.md`) reflect what is built, not what is planned

## Rust Practices

- Run all repository validation through `./build.sh`.
- Pass space-separated utility names only for routine scoped validation.
- Run `./build.sh` without targets before handoff and during release validation.
- Declare at least one Cargo binary target.
- Map each explicit Cargo binary to `src/bin/<utility>.rs`.
- Map each Cargo binary to `tests/<utility>_cli.rs`.
- Disable color in build-command tests that assert literal output.
- Keep Cargo compilation artifacts in the build-managed temporary target.
- Install all Cargo binary targets with tracked Cargo metadata during full builds.
- Install selected Cargo binaries with `--no-track --force` during scoped builds.
- Preserve unselected binaries and Cargo tracking metadata during scoped builds.
- Warn that scoped installation can overwrite a same-named selected binary.
- Install binaries only during successful post-change release validation.
- Skip binary installation during pre-change validation and `--no-build` release prep.
- Set `CARGO_HOME` to an external path when isolating binary installation.
- Resolve installed-binary name conflicts before rerunning the build.
- Format Rust code with rustfmt.
- Treat Clippy warnings as build failures.
- Test all targets and all features before handoff.
- Document public Rust items with rustdoc comments.
- Return contextual errors instead of discarding error sources.
- Confine `unsafe` code to the smallest practical scope.
- Document the safety invariant for every `unsafe` block.
- Prefer the standard library when it provides equivalent capability.
- Justify every added crate in the governing AC.
- Pin direct dependencies to explicit compatible versions in `Cargo.toml`.
- Keep `Cargo.lock` tracked for application repositories.

## Project Practices

- Follow existing repo patterns unless an approved improvement says otherwise.
