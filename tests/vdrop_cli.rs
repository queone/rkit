use std::process::Command;

#[test]
fn version_is_exact_and_terminal() {
    let output = Command::new(env!("CARGO_BIN_EXE_vdrop"))
        .arg("-v")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"vdrop v0.3.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_describes_crossfade_and_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_vdrop"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("vdrop START [END]"));
    assert!(text.contains("--crossfade"));
}

#[test]
fn malformed_positionals_fail_before_media_tools() {
    let output = Command::new(env!("CARGO_BIN_EXE_vdrop"))
        .args(["1:00"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("expected START"));
}
