use std::process::Command;

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_web"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"web v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_flag_prints_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_web"))
        .arg("-h")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("DuckDuckGo search utility"));
    assert!(text.contains("--open N"));
}

#[test]
fn empty_query_is_rejected_and_exits_one() {
    let output = Command::new(env!("CARGO_BIN_EXE_web"))
        .arg("   ")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("search query is empty"));
}

#[test]
fn no_query_is_rejected_and_exits_one() {
    let output = Command::new(env!("CARGO_BIN_EXE_web")).output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("search query is empty"));
}

#[test]
fn unknown_flag_is_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_web"))
        .args(["--bogus", "golang"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown flag"));
}

#[test]
fn invalid_open_index_is_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_web"))
        .args(["--open", "0", "golang"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("index must be 1 or greater"));
}

#[test]
fn documentation_describes_web_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## web"));
    assert!(readme.contains("web v2.0.0"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### web"));
}
