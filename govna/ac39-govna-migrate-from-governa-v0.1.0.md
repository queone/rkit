<!-- govna-migrate-from-governa: emitted-by govna v0.1.0; emission-sha=3a6f96540b07656e33a636630ac4f507396d938498db85b45bc6b341eab9406c -->
# AC39 Migrate From Governa

Track manual review and removal of the legacy `governa/` tree after `govna apply` wrote fresh govna canon alongside it.

## Summary

This repo was governa-managed. govna canon v0.1.0 has been applied. This AC tracks review of `governa/`; govna does not compare its contents against governa's canon beyond what's noted per item below. Nothing under `governa/` is deleted automatically.

### Routing Decisions

1. `governa/ac-template.md` is confirmed different from governa's current canon. Compare with: `governa render-canon --flavor code --stack Rust <scratch> && diff -ru <scratch>/governa/ac-template.md governa/ac-template.md`. Choose: delete canon-shape only, keep entirely, or delete entirely.
2. `governa/build-release.md` is confirmed different from governa's current canon. Compare with: `governa render-canon --flavor code --stack Rust <scratch> && diff -ru <scratch>/governa/build-release.md governa/build-release.md`. Choose: delete canon-shape only, keep entirely, or delete entirely.
3. `governa/drift-scan.md` is confirmed different from governa's current canon. Compare with: `governa render-canon --flavor code --stack Rust <scratch> && diff -ru <scratch>/governa/drift-scan.md governa/drift-scan.md`. Choose: delete canon-shape only, keep entirely, or delete entirely.
4. `governa/metadata.txt` is confirmed different from governa's current canon. Compare with: `governa render-canon --flavor code --stack Rust <scratch> && diff -ru <scratch>/governa/metadata.txt governa/metadata.txt`. Choose: delete canon-shape only, keep entirely, or delete entirely.

## In Scope

- `governa/README.md` — confirmed safe; confirmed byte-identical to governa's current canon.
- `governa/canon-cycle.md` — confirmed safe; confirmed byte-identical to governa's current canon.
- `governa/code-stacks.md` — confirmed safe; confirmed byte-identical to governa's current canon.
- `governa/development-cycle.md` — confirmed safe; confirmed byte-identical to governa's current canon.
- `governa/development-guidelines.md` — confirmed safe; confirmed byte-identical to governa's current canon.
- `governa/operator-contract-rationale.md` — confirmed safe; confirmed byte-identical to governa's current canon.
- `governa/roles.md` — confirmed safe; confirmed byte-identical to governa's current canon.

## Out Of Scope

- None.

## Acceptance Tests

**AT1** [Manual] — Director confirms every listed file was reviewed and either removed or intentionally kept.

**AT2** [Automated] [Pre-release gate] — `governa/` no longer exists in the repo.

## Status

`PENDING` — Emitted by `govna apply`; awaiting Director review.
