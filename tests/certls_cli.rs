use openssl::asn1::Asn1Time;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::ssl::{SslAcceptor, SslConnector, SslMethod, SslVerifyMode};
use openssl::x509::extension::SubjectAlternativeName;
use openssl::x509::{X509, X509NameBuilder};
use std::io::Read;
use std::net::TcpListener;
use std::process::Command;
use std::thread;

#[test]
fn version_and_argument_validation_are_terminal() {
    let version = Command::new(env!("CARGO_BIN_EXE_certls"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    assert_eq!(version.stdout, b"certls v2.0.0\n");
    let invalid = Command::new(env!("CARGO_BIN_EXE_certls"))
        .arg("a:b:c")
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&invalid.stdout).contains("Usage: certls FQDN[:PORT]"));
}

#[test]
fn inspects_a_deterministic_verified_local_tls_server() {
    let (key, certificate) = certificate();
    let mut acceptor = SslAcceptor::mozilla_intermediate(SslMethod::tls()).unwrap();
    acceptor.set_private_key(&key).unwrap();
    acceptor.set_certificate(&certificate).unwrap();
    acceptor.check_private_key().unwrap();
    let acceptor = acceptor.build();
    let listener = match TcpListener::bind(("127.0.0.1", 0)) {
        Ok(listener) => listener,
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => return,
        Err(error) => panic!("bind local TLS fixture: {error}"),
    };
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut stream = acceptor.accept(stream).unwrap();
        let mut buffer = [0_u8; 1];
        let _ = stream.read(&mut buffer);
    });

    let mut connector_builder = SslConnector::builder(SslMethod::tls()).unwrap();
    connector_builder.set_verify(SslVerifyMode::PEER);
    connector_builder
        .cert_store_mut()
        .add_cert(certificate.clone())
        .unwrap();
    let connector = connector_builder.build();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = rkit::certls::run_with_connector(
        [format!("localhost:{port}")],
        "2.0.0",
        &connector,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 0, "{}", String::from_utf8_lossy(&stderr));
    let text = String::from_utf8(stdout).unwrap();
    assert!(text.contains("==> TLS version"));
    assert!(text.contains(&format!("==> FQDN:Port localhost:{port}")));
    let expiry = text
        .lines()
        .find(|line| line.starts_with("==> EXPIRY: "))
        .unwrap();
    for field in expiry.split_whitespace().skip(2) {
        let value = field.split_once('=').unwrap().1;
        assert_eq!(value.len(), 20);
        assert!(value.starts_with("20"));
        assert_eq!(&value[4..5], "-");
        assert_eq!(&value[7..8], "-");
        assert_eq!(&value[10..11], "T");
        assert_eq!(&value[13..14], ":");
        assert_eq!(&value[16..17], ":");
        assert!(value.ends_with('Z'));
    }
    assert!(text.contains("localhost"));
    let _ = server.join();
}

fn certificate() -> (PKey<openssl::pkey::Private>, X509) {
    let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_text("CN", "localhost").unwrap();
    let name = name.build();
    let mut builder = X509::builder().unwrap();
    builder.set_version(2).unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&key).unwrap();
    let before = Asn1Time::days_from_now(0).unwrap();
    let after = Asn1Time::days_from_now(1).unwrap();
    builder.set_not_before(&before).unwrap();
    builder.set_not_after(&after).unwrap();
    let context = builder.x509v3_context(None, None);
    let san = SubjectAlternativeName::new()
        .dns("localhost")
        .build(&context)
        .unwrap();
    builder.append_extension(san).unwrap();
    builder.sign(&key, MessageDigest::sha256()).unwrap();
    (key, builder.build())
}
