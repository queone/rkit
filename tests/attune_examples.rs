use rkit::attune::spec;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn example_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/attune")
}

fn files_under(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn collect(root: &Path, directory: &Path, files: &mut Vec<(PathBuf, Vec<u8>)>) {
        let mut entries: Vec<_> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap())
            .collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                collect(root, &path, files);
            } else {
                files.push((
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                ));
            }
        }
    }

    let mut files = Vec::new();
    collect(root, root, &mut files);
    files
}

#[test]
fn public_example_validates_offline_without_writes() {
    let root = example_root();
    let before = files_under(&root);
    let output = Command::new(env!("CARGO_BIN_EXE_attune"))
        .arg("validate")
        .current_dir(&root)
        .env_remove("ARM_SUBSCRIPTION_ID")
        .env_remove("ARM_RESOURCE_GROUP")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"attune validate: OK (6 specs)\n");
    assert!(output.stderr.is_empty());
    assert_eq!(before, files_under(&root));
}

#[test]
fn public_example_has_one_of_every_supported_kind() {
    let bundle = spec::load(&example_root().join("specs")).unwrap();
    assert_eq!(bundle.len(), 6);
    assert_eq!(bundle.dns.len(), 1);
    assert_eq!(bundle.groups.len(), 1);
    assert_eq!(bundle.apps.len(), 1);
    assert_eq!(bundle.role_definitions.len(), 1);
    assert_eq!(bundle.role_assignments.len(), 1);
    assert_eq!(bundle.resource_groups.len(), 1);
}

#[test]
fn public_example_contains_only_safe_documentation_values() {
    let files = files_under(&example_root());
    for (_, bytes) in &files {
        let text = String::from_utf8_lossy(bytes);
        let lower = text.to_ascii_lowercase();
        for forbidden in [
            "/users/",
            "/home/",
            "c:\\",
            "-----begin certificate-----",
            "-----begin private key-----",
            "-----begin rsa private key-----",
        ] {
            assert!(!lower.contains(forbidden), "unsafe public example content");
        }
        for line in lower.lines() {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            if ["password", "secret", "token", "credential"]
                .iter()
                .any(|word| name.contains(word))
            {
                assert!(
                    value.trim().is_empty(),
                    "secret-shaped assignment in public example"
                );
            }
        }
    }

    let bundle = spec::load(&example_root().join("specs")).unwrap();
    for record in bundle.dns {
        assert!(is_reserved_name(&record.zone));
        if matches!(
            record.record_type.to_ascii_uppercase().as_str(),
            "CNAME" | "MX" | "NS"
        ) {
            for value in record.values {
                let host = value
                    .split_whitespace()
                    .last()
                    .unwrap_or("")
                    .trim_end_matches('.');
                assert!(is_reserved_name(host));
            }
        }
    }
}

fn is_reserved_name(name: &str) -> bool {
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    name == "example"
        || name.ends_with(".example")
        || name == "example.com"
        || name.ends_with(".example.com")
        || name == "example.net"
        || name.ends_with(".example.net")
        || name == "example.org"
        || name.ends_with(".example.org")
}
