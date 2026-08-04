//! XDG state persistence, legacy-path migration, and legacy-era pruning
//! for `cash5`, ported from Go's `store.go`.
//!
//! Path resolution takes an explicit [`Paths`] value rather than reading
//! `std::env` internally, so tests never mutate process-wide environment
//! state (`std::env::set_var` races across `cargo test`'s parallel test
//! threads — the exact hazard this repo's git history already names as a
//! "test flake" fix). Production code builds one real [`Paths`] via
//! [`Paths::from_env`]; tests build one pointing at a `TempDir`.

use crate::cash5::model::{self, Draw};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const PROGRAM_DATA_NAME: &str = "cash5";

/// UnixMilli of 2014-09-14 00:00:00 UTC, the first Cash 5 draw under the
/// 1-45 pool. Pre-cutoff data (1-40 era) is pruned at load.
pub const CASH5_ERA_START_MILLIS: i64 = 1_410_667_200_000;

/// The environment inputs `cash5`'s path resolution depends on.
pub struct Paths {
    pub home: PathBuf,
    pub xdg_state_home: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
}

impl Paths {
    /// Builds a `Paths` from the real process environment.
    pub fn from_env() -> io::Result<Self> {
        let home = env::var_os("HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .ok_or_else(|| io::Error::other("HOME is not set"))?;
        Ok(Self {
            home,
            xdg_state_home: env::var_os("XDG_STATE_HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
            xdg_config_home: env::var_os("XDG_CONFIG_HOME")
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        })
    }
}

fn xdg_state_dir(paths: &Paths) -> PathBuf {
    if let Some(value) = &paths.xdg_state_home
        && value.is_absolute()
    {
        return value.clone();
    }
    paths.home.join(".local").join("state")
}

fn xdg_config_dir(paths: &Paths) -> PathBuf {
    if let Some(value) = &paths.xdg_config_home
        && value.is_absolute()
    {
        return value.clone();
    }
    paths.home.join(".config")
}

/// Returns the canonical state-file path for the draws cache, performing a
/// one-shot lazy migration from the legacy `$HOME/.config/cash5/draws.json`
/// location on first call. Creates the parent directory so callers can
/// write directly.
pub fn config_path<E: Write>(paths: &Paths, stderr: &mut E) -> io::Result<PathBuf> {
    let new_dir = xdg_state_dir(paths).join(PROGRAM_DATA_NAME);
    let new_path = new_dir.join("draws.json");
    let old_path = xdg_config_dir(paths)
        .join(PROGRAM_DATA_NAME)
        .join("draws.json");

    if let Err(error) = migrate_if_needed(&old_path, &new_path, stderr) {
        let _ = writeln!(stderr, "{PROGRAM_DATA_NAME}: migration warning: {error}");
    }
    fs::create_dir_all(&new_dir)?;
    Ok(new_path)
}

/// Moves `old_path` to `new_path` when eligible: skip a symlinked
/// `old_path` with a warning, warn and prefer `new_path` when both exist,
/// and fall back to copy+delete (matching Go's explicit-0644 EXDEV
/// fallback) across a filesystem boundary.
fn migrate_if_needed<E: Write>(old_path: &Path, new_path: &Path, stderr: &mut E) -> io::Result<()> {
    let old_metadata = match fs::symlink_metadata(old_path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(()), // nothing to migrate
    };

    if old_metadata.file_type().is_symlink() {
        let _ = writeln!(
            stderr,
            "{PROGRAM_DATA_NAME}: {} is a symlink; skipping auto-migration. Move it to {} manually.",
            old_path.display(),
            new_path.display()
        );
        return Ok(());
    }

    if new_path.exists() {
        let _ = writeln!(
            stderr,
            "{PROGRAM_DATA_NAME}: both {} and {} exist; using {}. Delete the old file when ready.",
            old_path.display(),
            new_path.display(),
            new_path.display()
        );
        return Ok(());
    }

    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent)?;
    }

    match fs::rename(old_path, new_path) {
        Ok(()) => {}
        Err(error) if is_cross_device(&error) => {
            copy_file_go_style(old_path, new_path)?;
            fs::remove_file(old_path)?;
        }
        Err(error) => return Err(error),
    }

    let _ = writeln!(
        stderr,
        "{PROGRAM_DATA_NAME}: migrated {} -> {}",
        old_path.display(),
        new_path.display()
    );
    Ok(())
}

fn is_cross_device(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::CrossesDevices
}

/// Copies `src` to `dst` at a fixed `0o644`, matching Go's EXDEV
/// `copyFile` fallback (which does not preserve the source mode).
fn copy_file_go_style(src: &Path, dst: &Path) -> io::Result<()> {
    let content = fs::read(src)?;
    write_new_file(dst, &content)
}

#[cfg(unix)]
fn write_new_file(path: &Path, content: &[u8]) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o644)
        .open(path)?;
    file.write_all(content)
}

#[cfg(not(unix))]
fn write_new_file(path: &Path, content: &[u8]) -> io::Result<()> {
    fs::write(path, content)
}

/// Loads draws from `configPath()`, pruning the pre-2014-09-14 legacy era
/// and rewriting the file atomically (only) when something was pruned.
pub fn load_draws<E: Write>(paths: &Paths, stderr: &mut E) -> io::Result<Vec<Draw>> {
    let path = config_path(paths, stderr)?;
    let draws = match fs::read_to_string(&path) {
        Ok(text) => model::parse_draws_array(&text).map_err(io::Error::other)?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };

    let (pruned, removed) = prune_legacy_era(draws);
    if removed > 0 {
        if let Err(error) = atomic_write_draws(&path, &pruned) {
            let _ = writeln!(stderr, "{PROGRAM_DATA_NAME}: prune rewrite failed: {error}");
            return Ok(pruned);
        }
        let _ = writeln!(
            stderr,
            "{PROGRAM_DATA_NAME}: pruned {removed} pre-2014-09-14 rows from {}",
            path.display()
        );
    }
    Ok(pruned)
}

/// Filters out draws with `draw_time` before [`CASH5_ERA_START_MILLIS`].
/// Input order is preserved. Returns the post-cutoff slice and the count
/// removed.
pub fn prune_legacy_era(draws: Vec<Draw>) -> (Vec<Draw>, usize) {
    let before = draws.len();
    let kept: Vec<Draw> = draws
        .into_iter()
        .filter(|draw| draw.draw_time >= CASH5_ERA_START_MILLIS)
        .collect();
    let removed = before - kept.len();
    (kept, removed)
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `draws` to `path` via temp file + rename, so the write is atomic
/// on the same filesystem. The temp file is created in `path`'s own
/// directory and removed on any failure.
fn atomic_write_draws(path: &Path, draws: &[Draw]) -> io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| io::Error::other("path has no parent directory"))?;
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let tmp_path = dir.join(format!("draws-{}-{counter}.json.tmp", std::process::id()));
    let content = model::encode_draws(draws);
    if let Err(error) = fs::write(&tmp_path, content.as_bytes()) {
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }
    if let Err(error) = fs::rename(&tmp_path, path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(error);
    }
    Ok(())
}

/// Persists `draws` to `configPath()`'s resolved location, matching Go's
/// `saveDrawsCallback` (a plain truncate-write, not the atomic
/// temp-file-plus-rename path used only by the prune rewrite).
pub fn save_draws_callback<E: Write>(
    paths: &Paths,
    draws: &[Draw],
    stderr: &mut E,
) -> io::Result<()> {
    let path = config_path(paths, stderr)?;
    fs::write(&path, model::encode_draws(draws))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64 as TestCounter, Ordering as TestOrdering};

    static FIXTURE_NUMBER: TestCounter = TestCounter::new(0);

    fn clean_home() -> PathBuf {
        let number = FIXTURE_NUMBER.fetch_add(1, TestOrdering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rkit-cash5-unit-{}-{number}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn paths_for(home: &Path) -> Paths {
        Paths {
            home: home.to_path_buf(),
            xdg_state_home: None,
            xdg_config_home: None,
        }
    }

    #[test]
    fn xdg_state_dir_honors_absolute_override() {
        let home = clean_home();
        let absolute = home.join("custom-state");
        let mut paths = paths_for(&home);
        paths.xdg_state_home = Some(absolute.clone());
        assert_eq!(xdg_state_dir(&paths), absolute);
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn xdg_state_dir_ignores_relative_override() {
        let home = clean_home();
        let mut paths = paths_for(&home);
        paths.xdg_state_home = Some(PathBuf::from("relative/path"));
        assert_eq!(xdg_state_dir(&paths), home.join(".local").join("state"));
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn xdg_state_dir_falls_back_when_unset() {
        let home = clean_home();
        let paths = paths_for(&home);
        assert_eq!(xdg_state_dir(&paths), home.join(".local").join("state"));
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn config_path_resolves_new_location() {
        let home = clean_home();
        let paths = paths_for(&home);
        let mut stderr = Vec::new();
        let got = config_path(&paths, &mut stderr).unwrap();
        assert_eq!(
            got,
            home.join(".local")
                .join("state")
                .join("cash5")
                .join("draws.json")
        );
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn migration_moves_legacy_file() {
        let home = clean_home();
        let paths = paths_for(&home);
        let old_path = home.join(".config").join("cash5").join("draws.json");
        fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        let body = br#"[{"id":"draw-1","gameName":"Cash 5","drawTime":1735689600000}]"#;
        fs::write(&old_path, body).unwrap();

        let mut stderr = Vec::new();
        let new_path = config_path(&paths, &mut stderr).unwrap();
        assert!(!old_path.exists());
        assert_eq!(fs::read(&new_path).unwrap(), body);
        let stderr_text = String::from_utf8(stderr).unwrap();
        assert!(stderr_text.contains("migrated"));
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn migration_skipped_when_both_exist() {
        let home = clean_home();
        let paths = paths_for(&home);
        let old_path = home.join(".config").join("cash5").join("draws.json");
        let new_path = home
            .join(".local")
            .join("state")
            .join("cash5")
            .join("draws.json");
        fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        fs::create_dir_all(new_path.parent().unwrap()).unwrap();
        fs::write(&old_path, "OLD").unwrap();
        fs::write(&new_path, "NEW").unwrap();

        let mut stderr = Vec::new();
        config_path(&paths, &mut stderr).unwrap();
        assert!(String::from_utf8(stderr).unwrap().contains("both"));
        assert_eq!(fs::read_to_string(&new_path).unwrap(), "NEW");
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn migration_skipped_for_symlink() {
        let home = clean_home();
        let paths = paths_for(&home);
        let target = home.join("actual-draws.json");
        fs::write(&target, "[]").unwrap();
        let old_path = home.join(".config").join("cash5").join("draws.json");
        fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &old_path).unwrap();

        let mut stderr = Vec::new();
        config_path(&paths, &mut stderr).unwrap();
        assert!(old_path.is_symlink());
        assert!(String::from_utf8(stderr).unwrap().contains("symlink"));
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn cold_start_returns_empty_and_silent() {
        let home = clean_home();
        let paths = paths_for(&home);
        let mut stderr = Vec::new();
        let got = load_draws(&paths, &mut stderr).unwrap();
        assert!(got.is_empty());
        assert!(stderr.is_empty());
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn prune_legacy_era_filters_pre_cutoff() {
        let draws = vec![
            Draw {
                id: "pre-1".into(),
                draw_time: 718_430_400_000,
                ..Default::default()
            },
            Draw {
                id: "pre-2".into(),
                draw_time: CASH5_ERA_START_MILLIS - 86_400_000,
                ..Default::default()
            },
            Draw {
                id: "boundary".into(),
                draw_time: CASH5_ERA_START_MILLIS,
                ..Default::default()
            },
            Draw {
                id: "post-1".into(),
                draw_time: CASH5_ERA_START_MILLIS + 86_400_000,
                ..Default::default()
            },
        ];
        let (pruned, removed) = prune_legacy_era(draws);
        assert_eq!(removed, 2);
        assert_eq!(
            pruned.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
            vec!["boundary", "post-1"]
        );
    }

    #[test]
    fn prune_legacy_era_is_idempotent() {
        let draws = vec![Draw {
            id: "post-1".into(),
            draw_time: CASH5_ERA_START_MILLIS,
            ..Default::default()
        }];
        let (once, r1) = prune_legacy_era(draws);
        let (twice, r2) = prune_legacy_era(once.clone());
        assert_eq!((r1, r2), (0, 0));
        assert_eq!(once.len(), twice.len());
    }

    #[test]
    fn load_draws_prunes_and_rewrites_once() {
        let home = clean_home();
        let paths = paths_for(&home);
        let mut setup_stderr = Vec::new();
        let path = config_path(&paths, &mut setup_stderr).unwrap();

        let seed = vec![
            Draw {
                id: "pre-A".into(),
                draw_time: 718_430_400_000,
                ..Default::default()
            },
            Draw {
                id: "post-1".into(),
                draw_time: CASH5_ERA_START_MILLIS,
                ..Default::default()
            },
        ];
        atomic_write_draws(&path, &seed).unwrap();

        let mut stderr1 = Vec::new();
        let got = load_draws(&paths, &mut stderr1).unwrap();
        assert_eq!(got.len(), 1);
        assert!(
            String::from_utf8(stderr1)
                .unwrap()
                .contains("pruned 1 pre-2014-09-14 rows")
        );

        let modified_after_first = fs::metadata(&path).unwrap().modified().unwrap();

        let mut stderr2 = Vec::new();
        let got2 = load_draws(&paths, &mut stderr2).unwrap();
        assert_eq!(got2.len(), 1);
        assert!(stderr2.is_empty());
        let modified_after_second = fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(modified_after_first, modified_after_second);
        fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn save_and_load_round_trip_through_resolver() {
        let home = clean_home();
        let paths = paths_for(&home);
        let want = vec![Draw {
            id: "draw-1".into(),
            draw_time: 1_735_689_600_000,
            ..Default::default()
        }];
        let mut stderr = Vec::new();
        save_draws_callback(&paths, &want, &mut stderr).unwrap();

        let got = load_draws(&paths, &mut stderr).unwrap();
        assert_eq!(got.len(), want.len());
        assert_eq!(got[0].id, "draw-1");
        fs::remove_dir_all(&home).unwrap();
    }
}
