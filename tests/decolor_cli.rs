use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> std::path::PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("rkit-decolor-cli-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_decolor"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"decolor v1.1.1\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn decolorizes_file_bytes() {
    let directory = temp_dir();
    let path = directory.join("input.txt");
    fs::write(&path, b"\x1b[1;38;5;15mhello\x1b[0m\x00\xff\n").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_decolor"))
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"hello\x00\xff\n");
    assert!(output.stderr.is_empty());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn decolorizes_piped_input_and_ignores_extra_operands() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_decolor"))
        .args(["ignored", "operands"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"\x1b[38;2;30;144;255mblue\x1b[0m")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"blue");
    assert!(output.stderr.is_empty());
}

#[test]
fn documentation_describes_decolorizing_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## decolor"));
    assert!(readme.contains("decolor v1.1.1"));
    assert!(readme.contains("CSI SGR sequences (`ESC[...m`)"));
    assert!(readme.contains("stdin-read diagnostics are non-fatal"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### decolor"));
    assert!(arch.contains("Remove only CSI SGR sequences"));
}

#[test]
fn missing_file_reports_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_decolor"))
        .arg("missing-file")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Error reading file missing-file:"));
}
