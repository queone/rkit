use std::fs;
use std::path::Path;
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_attune"))
        .args(args)
        .env_remove("ARM_SUBSCRIPTION_ID")
        .env_remove("ARM_RESOURCE_GROUP")
        .output()
        .expect("run attune")
}

#[test]
fn version_is_build_compatible() {
    let output = run(&["--version"]);
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "attune 0.1.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn command_aliases_and_help_are_exposed() {
    for command in ["help", "h", "--help", "-h"] {
        let output = run(&[command]);
        assert!(output.status.success());
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("(p|plan)"));
        assert!(text.contains("-d, --diagnostic"));
    }
}

#[test]
fn validate_is_offline_and_creates_no_files() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/attune/specs");
    let before: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    let output = run(&["validate", "--specs", root.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "attune validate: OK (6 specs)\n"
    );
    let after: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(before, after);
}

#[test]
fn invalid_inputs_include_recovery_guidance() {
    let output = run(&["unknown"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("attune help"));
    let output = run(&["validate", "--unknown"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("attune help"));
}
