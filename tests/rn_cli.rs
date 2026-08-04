use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> std::path::PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-rn-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_rn"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"rn v1.5.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn dry_run_is_sorted_skips_directories_and_force_renames() {
    let directory = temp_dir();
    fs::write(directory.join("z_old.txt"), b"z").unwrap();
    fs::write(directory.join("a_old.txt"), b"a").unwrap();
    fs::create_dir(directory.join("folder_old")).unwrap();

    let dry = Command::new(env!("CARGO_BIN_EXE_rn"))
        .args(["old", "new"])
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(dry.status.success());
    let dry_text = String::from_utf8(dry.stdout).unwrap();
    assert!(dry_text.find("a_old.txt").unwrap() < dry_text.find("z_old.txt").unwrap());
    assert!(!directory.join("a_new.txt").exists());
    assert!(directory.join("folder_old").exists());

    let forced = Command::new(env!("CARGO_BIN_EXE_rn"))
        .args(["old", "new", "-f"])
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(forced.status.success());
    assert!(directory.join("a_new.txt").exists());
    assert!(directory.join("z_new.txt").exists());
    assert!(directory.join("folder_old").exists());
    assert!(!directory.join("a_old.txt").exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn no_match_returns_one() {
    let directory = temp_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_rn"))
        .args(["missing"])
        .current_dir(&directory)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("No filename has string 'missing'."));
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn force_rename_preserves_native_destination_replacement() {
    let directory = temp_dir();
    fs::write(directory.join("source.txt"), b"source").unwrap();
    fs::write(directory.join("target.txt"), b"target").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rn"))
        .args(["source", "target", "-f"])
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::read(directory.join("target.txt")).unwrap(), b"source");
    assert!(!directory.join("source.txt").exists());
    fs::remove_dir_all(directory).unwrap();
}
