use std::fs;
use std::path::Path;

#[test]
fn production_tls_connectors_use_the_shared_builder() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut violations = Vec::new();
    collect_direct_connector_calls(&source, &source.join("tls.rs"), &mut violations);
    assert!(
        violations.is_empty(),
        "production TLS connectors must use crate::tls: {}",
        violations.join(", ")
    );
}

fn collect_direct_connector_calls(directory: &Path, shared: &Path, violations: &mut Vec<String>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_direct_connector_calls(&path, shared, violations);
        } else if path.extension().and_then(|value| value.to_str()) == Some("rs")
            && path != shared
            && fs::read_to_string(&path)
                .unwrap()
                .contains("SslConnector::builder")
        {
            violations.push(
                path.strip_prefix(directory)
                    .unwrap_or(&path)
                    .display()
                    .to_string(),
            );
        }
    }
}
