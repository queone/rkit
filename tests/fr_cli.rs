use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-fr-cli-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"fr v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn bad_argument_count_prints_usage_to_stderr_and_exits_one() {
    let output = Command::new(env!("CARGO_BIN_EXE_fr")).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("search-only mode"));

    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["a", "b", "c", "d"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn unrecognized_third_argument_is_rejected() {
    let directory = temp_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["foo", "bar", "-x"])
        .current_dir(&directory)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Only -f/--force is supported"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn search_only_mode_prints_matching_lines_with_line_numbers() {
    let directory = temp_dir();
    fs::write(
        directory.join("note.txt"),
        "one\nfoo bar\nthree\nfoo again\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .arg("foo")
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("note.txt:2:"));
    assert!(text.contains("note.txt:4:"));
    assert!(!text.contains(":1:"));
    assert!(!text.contains(":3:"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn replace_and_write_mode_updates_files_and_reports_count_with_force_alias() {
    let directory = temp_dir();
    fs::write(directory.join("note.txt"), "foo\nfoo\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .args(["foo", "bar", "--force"])
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("2 occurrence(s) replaced"));
    assert_eq!(
        fs::read_to_string(directory.join("note.txt")).unwrap(),
        "bar\nbar\n"
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn nested_directories_are_walked_and_hidden_directories_are_skipped() {
    let directory = temp_dir();
    fs::create_dir(directory.join("sub")).unwrap();
    fs::write(directory.join("sub").join("deep.txt"), "needle\n").unwrap();
    fs::create_dir(directory.join(".hidden")).unwrap();
    fs::write(directory.join(".hidden").join("skip.txt"), "needle\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_fr"))
        .arg("needle")
        .current_dir(&directory)
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("sub/deep.txt"));
    assert!(!text.contains(".hidden"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn documentation_describes_fr_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## fr"));
    assert!(readme.contains("fr v2.0.0"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### fr"));
}
