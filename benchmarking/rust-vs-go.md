# Rust-versus-Go Benchmark Protocol

This is a living benchmark log and protocol for comparing the three rkit Rust
utilities with their Go counterparts:

- `tree`
- `dos2unix`
- `brew-update`

Each run appends one dated `Conclusion` block. Preserve earlier run data and
dates; apply presentation-only migrations only under an approved AC. A run is
valid only when its environment, artifacts,
workloads, raw samples, derived values, and deviations are recorded together.
Every `Conclusion` block must include one visible summary table with these
columns, in order: `Utility`, `Rust bytes`, `Go bytes`, `Rust size reduction`,
`Rust runtime`, `Go runtime`, `Rust runtime improvement`, `Rust clean build`,
and `Go clean build`. Use that run's artifact sizes and median standard-workload
results. Use `UNAVAILABLE` in a cell when the corresponding measurement could
not be made.

## Run contract

Use these configurable placeholders. Do not commit machine-specific absolute
paths:

```bash
export RKIT_ROOT="<rkit-root>"
export GO_UTILS_ROOT="<go-utils-root>"
export RUST_BIN_DIR="<rust-bin-dir>"
export GO_BIN_DIR="<go-bin-dir>"
```

For the original baseline, these resolved to the local rkit checkout, the Go
utilities checkout, and the installed Rust and Go binary directories. Future
agents must record the resolved values in the run block, but must not copy
machine-specific paths into this protocol as defaults.

Run the protocol from a macOS or Unix-like host with Rust, Cargo, Go, Perl,
`file`, `stat`, `shasum`, and `/usr/bin/time` available. Homebrew is never
required: the `brew-update` workload uses a temporary fake executable.

Abort with an actionable “unavailable measurement” record if a required
binary, source checkout, toolchain, dependency cache, or command is missing.
Do not substitute a different binary or silently skip a utility.

## Environment

Record this information before timing anything. Do not record machine names or
other machine identity values:

```bash
date -u '+%Y-%m-%dT%H:%M:%SZ'
sw_vers -productName
sw_vers -productVersion
uname -m
rustc --version
cargo --version
go version
git -C "$RKIT_ROOT" rev-parse HEAD
git -C "$GO_UTILS_ROOT" rev-parse HEAD
```

On non-macOS Unix hosts, record equivalent OS-release information and state the
substitutions in the run block. Do not record kernel machine identity values.

## Artifact identity

Verify all six binaries before measuring them:

```bash
for name in tree dos2unix brew-update; do
  file "$RUST_BIN_DIR/$name" "$GO_BIN_DIR/$name"
  stat -f '%N %z bytes' "$RUST_BIN_DIR/$name" "$GO_BIN_DIR/$name"
  shasum -a 256 "$RUST_BIN_DIR/$name" "$GO_BIN_DIR/$name"
done
```

On Linux, use `stat -c '%n %s bytes'` and `sha256sum` instead. Record each path,
architecture, byte size, checksum, and whether it is an installed artifact or
a freshly built artifact in a supporting artifact table. Use repository-safe
path placeholders instead of machine-specific absolute paths in committed
content. Do not compare artifacts with different architectures or optimization
modes without labeling the comparison as changed methodology.

## Workloads

Create one temporary fixture and remove it after the run:

```bash
bench_root=$(mktemp -d "${TMPDIR:-/tmp}/rkit-bench.XXXXXX")
mkdir -p "$bench_root/tree" "$bench_root/bin"
for i in $(seq 1 120); do touch "$bench_root/tree/file-$i"; done
for i in $(seq 1 20); do
  mkdir -p "$bench_root/tree/dir-$i"
  touch "$bench_root/tree/dir-$i/nested-$i"
done
dd if=/dev/zero of="$bench_root/input.bin" bs=1048576 count=8 2>/dev/null
ln -s /usr/bin/true "$bench_root/bin/brew"
```

The required baseline workloads are:

| Utility | Workload | Output policy |
|---|---|---|
| `tree` | Traverse 120 files and 20 one-file nested directories | Redirect stdout and stderr; leave color disabled by redirection |
| `dos2unix` | Preview the 8 MiB generated file without `--force` | Redirect stdout and stderr |
| `brew-update` | Run all four stages with fake `brew`; `brew list --cask` returns empty output | Set `PATH="$bench_root/bin:/usr/bin:/bin"`; redirect stdout and stderr |

Run the populated-cask supplemental workload separately with a fake `brew`
script that returns `alpha` and `beta` for `list --cask`. Record that it
tests the combined `brew upgrade alpha beta` path and is not part of the
baseline numbers unless explicitly labeled as a replacement methodology.

## Runtime

Use five samples. Each sample must batch enough invocations to exceed the
timer’s resolution, then divide the elapsed `real` time by the invocation
count. The following function is the reference harness; do not use a harness
that shifts away command arguments after the first iteration:

```bash
bench_batch() {
  label="$1"
  reps="$2"
  shift 2
  samples=''
  for sample in 1 2 3 4 5; do
    elapsed=$({
      /usr/bin/time -p sh -c '
        reps="$1"; shift
        i=0
        while [ "$i" -lt "$reps" ]; do
          "$@" >/dev/null 2>/dev/null
          i=$((i + 1))
        done
      ' sh "$reps" "$@"
    } 2>&1 | perl -ne 'print "$1\n" if /^real ([0-9.]+)/')
    samples="$samples$elapsed\n"
  done
  printf '%s raw-real-seconds=%b\n' "$label" "$samples"
}
```

Use these reference batch sizes:

```bash
bench_batch 'tree Rust' 100 "$RUST_BIN_DIR/tree" "$bench_root/tree"
bench_batch 'tree Go' 100 "$GO_BIN_DIR/tree" "$bench_root/tree"
bench_batch 'dos2unix Rust' 20 "$RUST_BIN_DIR/dos2unix" "$bench_root/input.bin"
bench_batch 'dos2unix Go' 20 "$GO_BIN_DIR/dos2unix" "$bench_root/input.bin"
bench_batch 'brew-update Rust' 20 env PATH="$bench_root/bin:/usr/bin:/bin" "$RUST_BIN_DIR/brew-update"
bench_batch 'brew-update Go' 20 env PATH="$bench_root/bin:/usr/bin:/bin" "$GO_BIN_DIR/brew-update"
```

Record all five raw samples. Report the median per-invocation milliseconds
and calculate Rust improvement as `100 * (Go - Rust) / Go`. Also run each
utility’s exact `--version` form in batches of 1,000 and record startup-only
medians separately; do not mix startup-only and workload timings.

## Compile time

Measure clean and warm builds separately. Use three samples for each utility
and record all raw `real` values.

For Rust, use a unique target directory for each clean sample:

```bash
env CARGO_TARGET_DIR="$temporary_target" \
  cargo build --release --bin "$name"
```

Run this from `$RKIT_ROOT`. For warm samples, build once into one target
directory, then repeat the same command three times without changing source
files.

For Go, run from `$GO_UTILS_ROOT` with a unique `GOCACHE` for every clean
sample:

```bash
env GOCACHE="$temporary_cache" \
    GOMODCACHE="$prewarmed_module_cache" \
    GOPROXY=off GOSUMDB=off \
  go build -trimpath -o "$temporary_output" "./cmd/$name"
```

Prewarm and verify `$prewarmed_module_cache` before starting the timer. Exclude
module download and dependency preparation from timed intervals. If a required
module is absent, record an unavailable measurement rather than allowing a
network download to change the result. Use the same warm-build rule as Rust.

Report clean and warm medians in seconds and calculate the Rust speedup as
`Go median / Rust median`.

## Binary size

Use the byte counts from Artifact identity. Report both absolute sizes and
`100 * (1 - Rust bytes / Go bytes)` for each utility. Size comparisons are
descriptive of the captured artifacts, not universal language-level claims.

## Maintainability

Do not assign a single unsupported maintainability score. Record evidence and
then a bounded interpretation for each dimension:

| Dimension | Evidence to record |
|---|---|
| Dependency surface | Manifest files, direct dependencies, and whether runtime dependencies are vendored or standard-library-only |
| Source and test size | Utility source lines, entrypoint lines, test lines, and documentation lines |
| Shared infrastructure | Shared error, color, argument, and build logic used by multiple utilities |
| Build workflow | Clean/warm build commands, package-wide checks, scoped checks, and installation behavior |
| Error handling | Context, exit codes, recovery guidance, and subprocess/file failure coverage |
| Regression coverage | Named automated tests and edge cases exercised |
| Onboarding cost | Required language/toolchain concepts and local setup steps |
| Update friction | Number of files and contracts normally touched for a behavior change |

Separate directly measured facts from judgment. State when a conclusion is
specific to this repository rather than inherent to Rust or Go.

## Failure rules

Mark a run `UNAVAILABLE` for a utility or dimension when a required input,
artifact, toolchain, dependency cache, or fake-command fixture is missing.
Record the exact command, error, and omitted dimension. Never replace missing
data with zero, an estimate, or a result from a different artifact.

Before accepting results, verify that:

- every raw sample has the expected count;
- all six artifacts have matching architecture and recorded identity;
- the fake Homebrew command performed no real update;
- the runtime harness preserved command arguments across every repetition;
- clean builds used isolated caches/targets;
- warm builds did not modify source files; and
- derived percentages and ratios can be recalculated from the recorded raw or median values.

## Raw samples

Create one table for every run containing the five runtime samples, five
startup-only samples, three clean compile samples, and three warm compile
samples for each utility/language pair. Keep the raw values even when a sample
is an outlier. Record batch size beside each runtime series and record
target/cache policy beside each compile series. Place each conclusion's complete
raw-sample table inside one closed `<details>` block with
`<summary>Raw samples</summary>` and no `open` attribute.

## Limitations

Results depend on host load, filesystem cache state, operating-system process
startup behavior, compiler versions, linker behavior, optimization flags,
artifact provenance, workload shape, and dependency-cache state. The baseline
uses installed arm64 macOS binaries, release builds, redirected output, and an
empty-cask fake Homebrew workflow. It does not measure real Homebrew network
latency, populated-cask parsing in the baseline, cross-compilation, or a
universal maintainability property.

## Conclusion — 2026-08-01 baseline

Run timestamp: `2026-08-01T14:50:26Z`. Environment: macOS
`26.6`; arm64; Rust `1.97.1`, Cargo
`1.97.1`, Go `1.26.5`; rkit revision
`428191e55f48258b1f8a6c7eb2239b5a2b216a7d`; Go utilities revision
`f403359f181bdf5c9e1f8ac04a0845673295e598`; release artifacts from the
configured installed binary directories; Rust Cargo release builds and Go
`-trimpath` builds. The benchmark used five runtime samples, five startup-only
samples, and three clean/warm compile samples, reporting medians.

<details>
<summary>Artifact identity</summary>

Artifact identity:

| Artifact | Path placeholder | Architecture | Bytes | SHA-256 | Provenance |
|---|---|---|---:|---|---|
| Rust `tree` | `$HOME/.cargo/bin/tree` | arm64 Mach-O | 475,744 | `42548f38b2f6d12c071f06939219b8c82b4c977b6189d025f276a85a7cbed1da` | Installed release |
| Rust `dos2unix` | `$HOME/.cargo/bin/dos2unix` | arm64 Mach-O | 456,144 | `317cbee2fd229e6cbdae373b1b8574b58a262f9c0f1b93a64a7582d84edaefe6` | Installed release |
| Rust `brew-update` | `$HOME/.cargo/bin/brew-update` | arm64 Mach-O | 498,464 | `cbdcad3db786f6adddc86b2788f57658f1e0db9579180e4c7e5f91a95387b47e` | Installed release |
| Go `tree` | `$GOPATH/bin/tree` | arm64 Mach-O | 1,960,882 | `ffa0742604774648d8c59977d1eef5d92c18e9e53af6ccee609a11752ed94d32` | Installed release |
| Go `dos2unix` | `$GOPATH/bin/dos2unix` | arm64 Mach-O | 1,691,074 | `f43b97d41e15b08f77f19e6db6ceb4e82ef235ce7dc02fa7f27e3d4966532e00` | Installed release |
| Go `brew-update` | `$GOPATH/bin/brew-update` | arm64 Mach-O | 1,860,898 | `df2456cbc8664ff9032b4fe50c97c35d55afcfb095cd2fdba0b8bdc3da039431` | Installed release |

</details>

<details>
<summary>Raw samples</summary>

Raw baseline samples follow. Runtime values are batch totals in seconds;
divide by 100 or 20 as specified above. Version values are batch totals for
1,000 invocations. Compile values are seconds.

| Measurement | Rust samples | Go samples |
|---|---|---|
| `tree` runtime, batch 100 | `0.19, 0.19, 0.19, 0.19, 0.19` | `0.24, 0.24, 0.24, 0.24, 0.24` |
| `dos2unix` runtime, batch 20 | `0.10, 0.10, 0.10, 0.10, 0.10` | `0.12, 0.12, 0.12, 0.12, 0.12` |
| `brew-update` runtime, batch 20 | `0.11, 0.11, 0.11, 0.11, 0.11` | `0.15, 0.16, 0.15, 0.16, 0.16` |
| `tree` version, batch 1,000 | `1.53, 1.53, 1.54, 1.52, 1.52` | `2.03, 2.02, 2.01, 2.00, 2.02` |
| `dos2unix` version, batch 1,000 | `1.54, 1.54, 1.54, 1.53, 1.53` | `1.98, 1.96, 1.97, 1.97, 1.97` |
| `brew-update` version, batch 1,000 | `1.60, 1.59, 1.59, 1.59, 1.58` | `2.03, 2.02, 2.02, 2.02, 2.02` |
| `tree` clean compile | `0.98, 0.45, 0.45` | `1.52, 1.52, 1.52` |
| `dos2unix` clean compile | `0.39, 0.41, 0.40` | `1.46, 1.46, 1.47` |
| `brew-update` clean compile | `0.40, 0.40, 0.40` | `1.50, 1.50, 1.50` |
| `tree` warm compile | `0.01, 0.01, 0.01` | `0.07, 0.08, 0.07` |
| `dos2unix` warm compile | `0.01, 0.01, 0.01` | `0.07, 0.07, 0.07` |
| `brew-update` warm compile | `0.01, 0.01, 0.01` | `0.08, 0.07, 0.08` |

</details>

| Utility | Rust bytes | Go bytes | Rust size reduction | Rust runtime | Go runtime | Rust runtime improvement | Rust clean build | Go clean build |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `tree` | 475,744 | 1,960,882 | 75.7% | 1.9 ms | 2.4 ms | 20.8% | 0.45 s | 1.52 s |
| `dos2unix` | 456,144 | 1,691,074 | 73.0% | 5.0 ms | 6.0 ms | 16.7% | 0.40 s | 1.46 s |
| `brew-update` | 498,464 | 1,860,898 | 73.2% | 5.5 ms | 8.0 ms | 31.3% | 0.40 s | 1.50 s |

Startup medians were 1.53/2.02 ms, 1.54/1.97 ms, and 1.59/2.02 ms for
`tree`, `dos2unix`, and `brew-update`, Rust/Go; warm medians were 0.01/0.07 s.
Rust was 17–31% faster, 73–76% smaller, and 3.4–3.8× faster to build; results
remain repository- and environment-specific.

## Conclusion — 2026-08-01 rerun

Run timestamp: `2026-08-01T14:55:47Z`. Environment: macOS
`26.6`; arm64; Rust `1.97.1`, Cargo `1.97.1`, Go `1.26.5`; rkit revision
`428191e55f48258b1f8a6c7eb2239b5a2b216a7d`; Go utilities revision
`f403359f181bdf5c9e1f8ac04a0845673295e598`; release artifacts from the
configured installed binary directories; Rust Cargo release builds and Go
`-trimpath` builds. The empty-cask baseline used five runtime samples, five
startup-only samples, and three clean/warm compile samples. The populated-cask
supplemental workload used five runtime samples. No source or binary artifact
changed from the prior baseline.

<details>
<summary>Artifact identity</summary>

Artifact identity was unchanged from the prior baseline:

| Artifact | Path placeholder | Architecture | Bytes | SHA-256 | Provenance |
|---|---|---|---:|---|---|
| Rust `tree` | `$HOME/.cargo/bin/tree` | arm64 Mach-O | 475,744 | `42548f38b2f6d12c071f06939219b8c82b4c977b6189d025f276a85a7cbed1da` | Installed release |
| Rust `dos2unix` | `$HOME/.cargo/bin/dos2unix` | arm64 Mach-O | 456,144 | `317cbee2fd229e6cbdae373b1b8574b58a262f9c0f1b93a64a7582d84edaefe6` | Installed release |
| Rust `brew-update` | `$HOME/.cargo/bin/brew-update` | arm64 Mach-O | 498,464 | `cbdcad3db786f6adddc86b2788f57658f1e0db9579180e4c7e5f91a95387b47e` | Installed release |
| Go `tree` | `$GOPATH/bin/tree` | arm64 Mach-O | 1,960,882 | `ffa0742604774648d8c59977d1eef5d92c18e9e53af6ccee609a11752ed94d32` | Installed release |
| Go `dos2unix` | `$GOPATH/bin/dos2unix` | arm64 Mach-O | 1,691,074 | `f43b97d41e15b08f77f19e6db6ceb4e82ef235ce7dc02fa7f27e3d4966532e00` | Installed release |
| Go `brew-update` | `$GOPATH/bin/brew-update` | arm64 Mach-O | 1,860,898 | `df2456cbc8664ff9032b4fe50c97c35d55afcfb095cd2fdba0b8bdc3da039431` | Installed release |

</details>

<details>
<summary>Raw samples</summary>

Raw rerun samples are batch totals in seconds. Runtime batches were 100 for
`tree` and 20 for `dos2unix` and `brew-update`; version batches were 1,000.
Compile values are seconds.

| Measurement | Rust samples | Go samples |
|---|---|---|
| `tree` runtime, batch 100 | `0.20, 0.19, 0.19, 0.20, 0.20` | `0.25, 0.24, 0.25, 0.25, 0.25` |
| `dos2unix` runtime, batch 20 | `0.10, 0.10, 0.10, 0.10, 0.10` | `0.12, 0.12, 0.12, 0.12, 0.12` |
| `brew-update` empty cask, batch 20 | `0.11, 0.11, 0.11, 0.11, 0.11` | `0.16, 0.16, 0.15, 0.16, 0.16` |
| `brew-update` populated casks, batch 20 | `0.58, 0.26, 0.26, 0.26, 0.26` | `0.32, 0.32, 0.32, 0.31, 0.32` |
| `tree` version, batch 1,000 | `1.52, 1.53, 1.53, 1.53, 1.53` | `2.00, 2.05, 2.07, 2.03, 1.96` |
| `dos2unix` version, batch 1,000 | `1.53, 1.57, 1.55, 1.51, 1.51` | `1.93, 1.93, 1.91, 1.94, 2.02` |
| `brew-update` version, batch 1,000 | `1.59, 1.60, 1.75, 1.63, 1.62` | `2.10, 2.16, 2.10, 1.99, 1.97` |
| `tree` clean compile | `0.45, 0.45, 0.45` | `1.52, 1.54, 1.53` |
| `dos2unix` clean compile | `0.39, 0.39, 0.39` | `1.45, 1.47, 1.46` |
| `brew-update` clean compile | `0.40, 0.40, 0.40` | `1.50, 1.51, 1.52` |
| `tree` warm compile | `0.01, 0.01, 0.01` | `0.07, 0.07, 0.07` |
| `dos2unix` warm compile | `0.01, 0.01, 0.01` | `0.07, 0.07, 0.07` |
| `brew-update` warm compile | `0.01, 0.01, 0.01` | `0.08, 0.07, 0.07` |

</details>

| Utility | Rust bytes | Go bytes | Rust size reduction | Rust runtime | Go runtime | Rust runtime improvement | Rust clean build | Go clean build |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `tree` | 475,744 | 1,960,882 | 75.7% | 2.0 ms | 2.5 ms | 20.0% | 0.45 s | 1.53 s |
| `dos2unix` | 456,144 | 1,691,074 | 73.0% | 5.0 ms | 6.0 ms | 16.7% | 0.39 s | 1.46 s |
| `brew-update` | 498,464 | 1,860,898 | 73.2% | 5.5 ms | 8.0 ms | 31.3% | 0.40 s | 1.51 s |

Startup medians were 1.53/2.03 ms, 1.53/1.93 ms, and 1.62/2.10 ms;
populated-cask `brew-update` was 13/16 ms, Rust/Go. Rust remained 17–31%
faster and 73–76% smaller; a Go cache-permission warning was recorded.

## Conclusion — 2026-08-01 machine run

Run timestamp: `2026-08-01T16:01:16Z`. Environment: macOS
`26.6`; arm64; Rust `1.97.1`, Cargo `1.97.1`, Go `1.26.5`; rkit revision
`3b26af4888058829f7adf1495a313925dc45845f`; Go utilities revision
`f403359f181bdf5c9e1f8ac04a0845673295e598`. Resolved artifact locations were
`$HOME/.cargo/bin` for Rust and `$GOPATH/bin` for Go; source checkouts were
the rkit root and its sibling Go utilities checkout. Artifacts were installed
release binaries, all arm64 Mach-O executables. The baseline used five runtime
samples, five startup-only samples, and three clean/warm compile samples.

<details>
<summary>Artifact identity</summary>

Artifact identity:

| Artifact | Path placeholder | Architecture | Bytes | SHA-256 | Provenance |
|---|---|---|---:|---|---|
| Rust `tree` | `$HOME/.cargo/bin/tree` | arm64 Mach-O | 475,744 | `988d8e4ec2f443d67e8c9284bce8199bae4921f1f452c48e8a263a477803ecf5` | Installed release |
| Rust `dos2unix` | `$HOME/.cargo/bin/dos2unix` | arm64 Mach-O | 456,144 | `bd12d2c9a82ba79a29c2d965ce8880f44819c15738d57c634cce6a8cf490ebee` | Installed release |
| Rust `brew-update` | `$HOME/.cargo/bin/brew-update` | arm64 Mach-O | 498,464 | `75649666442186910488499b450e8478d97cd9586db1e95ad27aed7b9b65de94` | Installed release |
| Go `tree` | `$GOPATH/bin/tree` | arm64 Mach-O | 1,960,882 | `211afe89c50e98d9af1644a02788dfae72adf02c4880f80acd9e0c0f0c02805b` | Installed release |
| Go `dos2unix` | `$GOPATH/bin/dos2unix` | arm64 Mach-O | 1,691,074 | `723807b4cacfddb452fc1f156ee122e3245bd4fcf3ff7e8d944015da2c587cbd` | Installed release |
| Go `brew-update` | `$GOPATH/bin/brew-update` | arm64 Mach-O | 1,860,898 | `9c6d30b4b5401dd4a5491a71f943028f3ba5c10f0a7f0de8621641706deaa795` | Installed release |

</details>

<details>
<summary>Raw samples</summary>

Raw samples are batch totals in seconds. Runtime batches were 100 for
`tree`, 20 for `dos2unix` and `brew-update`; startup-only batches were 1,000.
Compile values are seconds.

| Measurement | Rust samples | Go samples |
|---|---|---|
| `tree` runtime, batch 100 | `0.20, 0.20, 0.20, 0.20, 0.20` | `0.25, 0.25, 0.25, 0.25, 0.25` |
| `dos2unix` runtime, batch 20 | `0.11, 0.11, 0.11, 0.11, 0.11` | `0.12, 0.12, 0.12, 0.12, 0.12` |
| `brew-update` runtime, batch 20 | `0.12, 0.11, 0.11, 0.11, 0.11` | `0.15, 0.16, 0.15, 0.15, 0.15` |
| `tree` version, batch 1,000 | `1.59, 1.58, 1.61, 1.57, 1.58` | `2.00, 2.05, 2.05, 2.04, 2.01` |
| `dos2unix` version, batch 1,000 | `1.59, 1.59, 1.59, 1.59, 1.59` | `2.01, 2.00, 2.01, 2.00, 2.00` |
| `brew-update` version, batch 1,000 | `1.66, 1.66, 1.66, 1.66, 1.66` | `2.08, 2.08, 2.08, 2.08, 2.08` |
| `tree` clean compile | `0.82, 0.48, 0.48` | `2.01, 1.88, 1.88` |
| `dos2unix` clean compile | `0.41, 0.41, 0.41` | `1.81, 1.81, 1.81` |
| `brew-update` clean compile | `0.42, 0.42, 0.42` | `1.81, 1.80, 1.80` |
| `tree` warm compile | `0.01, 0.01, 0.01` | `0.07, 0.07, 0.07` |
| `dos2unix` warm compile | `0.01, 0.01, 0.01` | `0.07, 0.07, 0.07` |
| `brew-update` warm compile | `0.01, 0.01, 0.01` | `0.07, 0.07, 0.07` |

</details>

| Utility | Rust bytes | Go bytes | Rust size reduction | Rust runtime | Go runtime | Rust runtime improvement | Rust clean build | Go clean build |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `tree` | 475,744 | 1,960,882 | 75.7% | 2.0 ms | 2.5 ms | 20.0% | 0.48 s | 1.88 s |
| `dos2unix` | 456,144 | 1,691,074 | 73.0% | 5.5 ms | 6.0 ms | 8.3% | 0.41 s | 1.81 s |
| `brew-update` | 498,464 | 1,860,898 | 73.2% | 5.5 ms | 7.5 ms | 26.7% | 0.42 s | 1.80 s |

Startup medians were 1.58/2.04 ms, 1.59/2.00 ms, and 1.66/2.08 ms;
warm medians were 0.01/0.07 s, Rust/Go. Rust was 8–27% faster and 73–76%
smaller; one discarded Go compile attempt used the wrong checkout and was not
included in the results.
