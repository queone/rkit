use std::process::Command;

#[test]
fn version_is_exact_and_terminal() {
    let output = Command::new(env!("CARGO_BIN_EXE_vkeep"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"vkeep v0.3.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_and_invalid_flags_are_deterministic() {
    let help = Command::new(env!("CARGO_BIN_EXE_vkeep"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("vkeep START [END]"));

    let invalid = Command::new(env!("CARGO_BIN_EXE_vkeep"))
        .args(["--bad", "0", "input.mp4"])
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unknown flag"));
}

#[test]
fn crossfade_is_rejected_by_vkeep_before_media_tools() {
    let output = Command::new(env!("CARGO_BIN_EXE_vkeep"))
        .args(["-x", "1:00", "5:00", "input.mp4"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("applies only to vdrop"));
}
