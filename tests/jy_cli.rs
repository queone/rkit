use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-jy-cli-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

fn run_piped(args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_jy"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_jy"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"jy v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_flag_prints_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_jy"))
        .arg("-h")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("JSON / YAML converter"));
}

#[test]
fn json_file_converts_to_yaml_and_yaml_file_converts_to_json() {
    let directory = temp_dir();
    let json_path = directory.join("input.json");
    fs::write(&json_path, br#"{"name": "rkit", "count": 3}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_jy"))
        .args([json_path.to_str().unwrap(), "-d"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("name: rkit"));
    assert!(text.contains("count: 3"));

    let yaml_path = directory.join("input.yaml");
    fs::write(&yaml_path, "name: rkit\ncount: 3\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_jy"))
        .args([yaml_path.to_str().unwrap(), "-d"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("\"name\": \"rkit\""));
    assert!(text.contains("\"count\": 3"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn piped_input_is_converted_and_ansi_stripped_before_parsing() {
    let colored_json = b"\x1b[32m{\"a\": 1}\x1b[0m";
    let output = run_piped(&["-d"], colored_json);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("a: 1"));
}

#[test]
fn colorize_flag_prints_raw_file_content_without_converting() {
    let directory = temp_dir();
    let json_path = directory.join("input.json");
    fs::write(&json_path, br#"{"a": 1}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_jy"))
        .args([json_path.to_str().unwrap(), "-c"])
        .output()
        .unwrap();
    assert!(output.status.success());
    // Not piped through a terminal, so color is disabled and the file's
    // own (JSON) syntax is printed back unconverted.
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        r#"{"a": 1}"#
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn colorize_flag_on_missing_file_reports_neither_format() {
    // Matches the Go original: `-c` routes read failures through the same
    // "neither format" diagnostic as a genuinely invalid file.
    let directory = temp_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_jy"))
        .args([directory.join("missing.json").to_str().unwrap(), "-c"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("File is neither JSON nor YAML"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn invalid_input_reports_not_json_nor_yaml() {
    let output = run_piped(&[], b": : :not valid at all: [[[");
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Not JSON nor YAML"));
}

#[test]
fn missing_file_reports_unusable() {
    let directory = temp_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_jy"))
        .arg(directory.join("missing.json").to_str().unwrap())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("File is unusable"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn documentation_describes_jy_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## jy"));
    assert!(readme.contains("jy v2.0.0"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### jy"));
}
