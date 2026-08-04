use openssl::pkey::PKey;
use openssl::x509::{X509, X509Req};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> std::path::PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-certgen-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_certgen"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"certgen v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn generates_rsa_certificate_csr_and_key_with_expected_metadata() {
    let directory = temp_dir();
    let mut command = Command::new(env!("CARGO_BIN_EXE_certgen"));
    command
        .arg("example.test")
        .current_dir(&directory)
        .stdin(std::process::Stdio::piped());
    let output = command.output_with_input(b"Y\n").unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let key_bytes = fs::read(directory.join("example.test.key")).unwrap();
    assert!(key_bytes.starts_with(b"-----BEGIN RSA PRIVATE KEY-----"));
    let key = PKey::private_key_from_pem(&key_bytes).unwrap();
    assert_eq!(key.bits(), 2048);
    let csr_bytes = fs::read(directory.join("example.test.csr")).unwrap();
    assert!(csr_bytes.starts_with(b"-----BEGIN CERTIFICATE REQUEST-----"));
    let csr = X509Req::from_pem(&csr_bytes).unwrap();
    assert_eq!(
        csr.subject_name()
            .entries_by_nid(openssl::nid::Nid::COMMONNAME)
            .next()
            .unwrap()
            .data()
            .as_utf8()
            .unwrap()
            .to_string(),
        "example.test"
    );
    let csr_text = String::from_utf8(csr.to_text().unwrap()).unwrap();
    assert!(csr_text.contains("DNS:example.test"));
    let certificate_bytes = fs::read(directory.join("example.test.crt")).unwrap();
    assert!(certificate_bytes.starts_with(b"-----BEGIN CERTIFICATE-----"));
    let certificate = X509::from_pem(&certificate_bytes).unwrap();
    assert_eq!(
        certificate
            .subject_name()
            .entries_by_nid(openssl::nid::Nid::COMMONNAME)
            .next()
            .unwrap()
            .data()
            .as_utf8()
            .unwrap()
            .to_string(),
        "example.test"
    );
    assert!(
        certificate
            .subject_alt_names()
            .unwrap()
            .iter()
            .any(|name| name.dnsname() == Some("example.test"))
    );
    let certificate_text = String::from_utf8(certificate.to_text().unwrap()).unwrap();
    assert!(certificate_text.contains("Digital Signature"));
    assert!(certificate_text.contains("TLS Web Server Authentication"));
    assert!(
        output
            .stdout
            .windows(b"example.test.crt".len())
            .any(|window| window == b"example.test.crt")
    );
    #[cfg(unix)]
    {
        assert_eq!(
            fs::metadata(directory.join("example.test.key"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(directory.join("example.test.csr"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        assert_eq!(
            fs::metadata(directory.join("example.test.crt"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
    }
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn declines_with_nonzero_status_without_artifacts() {
    let directory = temp_dir();
    let mut child = Command::new(env!("CARGO_BIN_EXE_certgen"))
        .arg("example.test")
        .current_dir(&directory)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"N\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.ends_with(b"\nAborted.\n"));
    assert!(!directory.join("example.test.key").exists());
    fs::remove_dir_all(directory).unwrap();
}

trait OutputWithInput {
    fn output_with_input(&mut self, input: &[u8]) -> std::io::Result<std::process::Output>;
}

impl OutputWithInput for std::process::Command {
    fn output_with_input(&mut self, input: &[u8]) -> std::io::Result<std::process::Output> {
        self.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = self.spawn()?;
        use std::io::Write;
        child.stdin.take().unwrap().write_all(input)?;
        child.wait_with_output()
    }
}
