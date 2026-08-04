use std::process::Command;

#[test]
fn version_is_exact_and_terminal() {
    let output = Command::new(env!("CARGO_BIN_EXE_vjoin"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"vjoin v0.1.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_and_invalid_arguments_are_deterministic() {
    let help = Command::new(env!("CARGO_BIN_EXE_vjoin"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("vjoin INPUT1 INPUT2"));

    let invalid = Command::new(env!("CARGO_BIN_EXE_vjoin"))
        .arg("one.mp4")
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("expected INPUT1 INPUT2"));
}
