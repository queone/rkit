use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    assert_eq!(String::from_utf8_lossy(&output.stdout), "attune 0.1.1\n");
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

fn validate_synthetic_spec(yaml: &str) -> Output {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "rkit-attune-principal-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&root).unwrap();
    fs::write(root.join("role-assignment.yaml"), yaml).unwrap();
    let before: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();

    let output = run(&["validate", "--specs", root.to_str().unwrap()]);

    let after: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(before, after);
    fs::remove_dir_all(root).unwrap();
    output
}

#[test]
fn legacy_principal_forms_validate_offline_without_artifacts() {
    let security_group = validate_synthetic_spec(
        "kind: roleAssignment\nprincipal: synthetic-group\nprincipalType: SeCuRiTyGrOuP\nrole: synthetic-role\nscope:\n  resourceGroup: synthetic-resources\n",
    );
    assert!(
        security_group.status.success(),
        "{}",
        String::from_utf8_lossy(&security_group.stderr)
    );

    let literal_id = validate_synthetic_spec(
        "kind: roleAssignment\nprincipal: 00000000-0000-0000-0000-000000000004\nrole: synthetic-role\nscope:\n  resourceGroup: synthetic-resources\n",
    );
    assert!(
        literal_id.status.success(),
        "{}",
        String::from_utf8_lossy(&literal_id.stderr)
    );
}

#[test]
fn named_principal_without_type_has_recovery_guidance() {
    let output = validate_synthetic_spec(
        "kind: roleAssignment\nprincipal: synthetic-group\nrole: synthetic-role\nscope:\n  resourceGroup: synthetic-resources\n",
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    for principal_type in ["group", "securityGroup", "servicePrincipal", "user"] {
        assert!(error.contains(principal_type), "{error}");
    }
}
