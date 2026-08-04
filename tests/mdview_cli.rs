use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("rkit-mdview-cli-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn version_is_terminal_and_exact() {
    // Deliberately diverges from the Go original, which folded -v into the
    // same full-usage-screen path as -h: this repo's build.sh requires
    // every utility's --version output to be exactly `name vX.Y.Z`.
    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"mdview v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_flag_prints_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg("-h")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("-o, --output FILE"));
}

#[test]
fn no_arguments_prints_usage_and_exits_zero() {
    let output = Command::new(env!("CARGO_BIN_EXE_mdview")).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("View GitHub Flavored Markdown"));
}

#[test]
fn output_flag_writes_html_and_refuses_existing_destination() {
    let directory = temp_dir();
    let input = directory.join("input.md");
    fs::write(&input, "# Hello\n\n- [x] done\n").unwrap();
    let output_path = directory.join("page.html");

    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .args([input.to_str().unwrap(), "-o", output_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .starts_with(&format!("Created: {}", output_path.display()))
    );
    let html = fs::read_to_string(&output_path).unwrap();
    assert!(html.contains("<body class=\"markdown-body\">"));
    assert!(html.contains("type=\"checkbox\""));

    let repeat = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .args([
            input.to_str().unwrap(),
            "--output",
            output_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(repeat.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&repeat.stderr).contains("already exists"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn inline_equals_output_forms_are_accepted() {
    let directory = temp_dir();
    let input = directory.join("input.md");
    fs::write(&input, "# Hello").unwrap();
    let output_path = directory.join("out.html");

    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .arg(format!("-o={}", output_path.display()))
        .arg(&input)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output_path.exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn missing_input_reports_diagnostic_and_exits_one() {
    let directory = temp_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .args([
            directory.join("missing.md").to_str().unwrap(),
            "-o",
            directory.join("out.html").to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("provide an existing readable file"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn unknown_flag_is_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_mdview"))
        .args(["--bogus", "file.md"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown flag"));
}

#[test]
fn documentation_describes_mdview_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## mdview"));
    assert!(readme.contains("mdview v2.0.0"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### mdview"));
}
