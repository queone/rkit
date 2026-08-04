use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> std::path::PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-bak-cli-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

fn backup_path(directory: &std::path::Path, prefix: &str) -> std::path::PathBuf {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(prefix)
        })
        .unwrap()
}

fn utc_yyyymmdd() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y%m%d"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn backup_falls_back_to_utc_without_the_date_command() {
    let directory = temp_dir();
    let source = directory.join("source.txt");
    fs::write(&source, b"payload").unwrap();
    let before = utc_yyyymmdd();
    let output = Command::new(env!("CARGO_BIN_EXE_bak"))
        .env("PATH", "")
        .arg(&source)
        .output()
        .unwrap();
    let after = utc_yyyymmdd();
    assert!(output.status.success());
    let backup = backup_path(&directory, "source.txt.20");
    let name = backup.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name == format!("source.txt.{before}") || name == format!("source.txt.{after}"),
        "backup {name}, expected suffix {before} or {after}"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_bak"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"bak v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn backs_up_files_and_uses_alphabetic_collision_suffixes() {
    let directory = temp_dir();
    let source = directory.join("source.txt");
    fs::write(&source, b"payload").unwrap();

    let first = Command::new(env!("CARGO_BIN_EXE_bak"))
        .arg(&source)
        .output()
        .unwrap();
    assert!(first.status.success());
    assert!(first.stdout.is_empty());
    assert!(first.stderr.is_empty());

    let first_backup = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("source.txt.20")
        })
        .unwrap();
    assert_eq!(fs::read(&first_backup).unwrap(), b"payload");

    let second = Command::new(env!("CARGO_BIN_EXE_bak"))
        .arg(&source)
        .output()
        .unwrap();
    assert!(second.status.success());
    let mut backups: Vec<_> = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("source.txt.20")
        })
        .collect();
    backups.sort();
    assert_eq!(backups.len(), 2);
    assert!(backups[1].to_string_lossy().ends_with('a'));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn documentation_describes_backup_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## bak"));
    assert!(readme.contains("bak v2.0.0"));
    assert!(readme.contains("Usage: bak <file|directory>"));
    assert!(readme.contains("(`a`, `b`, …, `z`, `aa`, …)"));
    assert!(readme.contains("falls back to the UTC calendar date"));
    assert!(readme.contains("backup failed:"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### bak"));
    assert!(arch.contains("host `date` command with a UTC fallback"));
    assert!(arch.contains("do not add a runtime dependency for these utilities"));
}

#[test]
fn missing_source_prints_usage_and_fails() {
    let output = Command::new(env!("CARGO_BIN_EXE_bak"))
        .arg("missing-source")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"Usage: bak <file|directory>\n");
    assert!(output.stderr.is_empty());
}

#[cfg(unix)]
#[test]
fn directory_symlinks_are_traversed_to_their_targets() {
    let directory = temp_dir();
    let source = directory.join("tree");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("real"), b"content").unwrap();
    std::os::unix::fs::symlink(source.join("real"), source.join("link")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bak"))
        .arg(&source)
        .output()
        .unwrap();
    assert!(output.status.success());
    let backup = backup_path(&directory, "tree.20");
    let copied_link = backup.join("link");
    assert!(
        fs::symlink_metadata(&copied_link)
            .unwrap()
            .file_type()
            .is_file()
    );
    assert_eq!(fs::read(copied_link).unwrap(), b"content");
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn dangling_symlink_fails_and_leaves_partial_backup() {
    let directory = temp_dir();
    let source = directory.join("tree");
    fs::create_dir(&source).unwrap();
    std::os::unix::fs::symlink(source.join("missing"), source.join("dangling")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bak"))
        .arg(&source)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("backup failed:"));
    assert!(backup_path(&directory, "tree.20").is_dir());
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn unreadable_file_fails_and_leaves_partial_backup() {
    use std::os::unix::fs::PermissionsExt;
    let directory = temp_dir();
    let source = directory.join("tree");
    fs::create_dir(&source).unwrap();
    let secret = source.join("secret");
    fs::write(&secret, b"secret").unwrap();
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bak"))
        .arg(&source)
        .output()
        .unwrap();
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("backup failed:"));
    assert!(backup_path(&directory, "tree.20").is_dir());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn recursively_copies_directories() {
    let directory = temp_dir();
    let source = directory.join("tree");
    fs::create_dir(&source).unwrap();
    fs::create_dir(source.join("nested")).unwrap();
    fs::write(source.join("nested/file"), b"nested").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_bak"))
        .arg(&source)
        .output()
        .unwrap();
    assert!(output.status.success());
    let backup = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("tree.20")
        })
        .unwrap();
    assert_eq!(fs::read(backup.join("nested/file")).unwrap(), b"nested");
    fs::remove_dir_all(directory).unwrap();
}
