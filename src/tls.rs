//! Shared verified TLS connector construction and platform trust loading.

use openssl::ssl::{SslConnector, SslConnectorBuilder, SslMethod, SslVerifyMode};
use openssl::x509::X509;
#[cfg(target_os = "macos")]
use std::process::Command;

/// Build a peer-verifying TLS connector from OpenSSL and platform trust.
pub fn connector() -> Result<SslConnector, String> {
    let mut builder = SslConnector::builder(SslMethod::tls())
        .map_err(|error| format!("create TLS connector: {error}"))?;
    builder.set_verify(SslVerifyMode::PEER);
    configure_trust(&mut builder)?;
    Ok(builder.build())
}

fn configure_trust(builder: &mut SslConnectorBuilder) -> Result<(), String> {
    let default_error = builder
        .set_default_verify_paths()
        .err()
        .map(|error| error.to_string());

    #[cfg(target_os = "macos")]
    {
        let keychain_error = load_macos_keychain_roots(builder).err();
        if default_error.is_some() && keychain_error.is_some() {
            return Err(format!(
                "load trusted certificate paths: default paths: {}; macOS keychain: {}",
                default_error.unwrap_or_default(),
                keychain_error.unwrap_or_default()
            ));
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    default_error.map_or(Ok(()), |error| {
        Err(format!("load trusted certificate paths: {error}"))
    })
}

#[cfg(target_os = "macos")]
fn load_macos_keychain_roots(builder: &mut SslConnectorBuilder) -> Result<usize, String> {
    let keychains = [
        "/System/Library/Keychains/SystemRootCertificates.keychain",
        "/Library/Keychains/System.keychain",
    ];
    let mut loaded = 0;
    let mut last_error = None;
    for path in keychains {
        let output = match Command::new("/usr/bin/security")
            .args(["find-certificate", "-a", "-p", path])
            .output()
        {
            Ok(output) => output,
            Err(error) => {
                last_error = Some(error.to_string());
                continue;
            }
        };
        if !output.status.success() {
            last_error = Some(format!("security exited with {}", output.status));
            continue;
        }
        match add_pem_certificates(builder, &output.stdout) {
            Ok(count) => loaded += count,
            Err(error) => last_error = Some(error),
        }
    }
    if loaded == 0 {
        return Err(last_error.unwrap_or_else(|| "no macOS keychain certificates found".into()));
    }
    Ok(loaded)
}

fn add_pem_certificates(builder: &mut SslConnectorBuilder, pem: &[u8]) -> Result<usize, String> {
    let certificates = X509::stack_from_pem(pem)
        .map_err(|error| format!("parse trusted certificates: {error}"))?;
    if certificates.is_empty() {
        return Err("parse trusted certificates: no certificates found".into());
    }
    let mut loaded = 0;
    for certificate in certificates {
        if builder.cert_store_mut().add_cert(certificate).is_ok() {
            loaded += 1;
        }
    }
    Ok(loaded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openssl::asn1::Asn1Time;
    use openssl::hash::MessageDigest;
    use openssl::pkey::PKey;
    use openssl::rsa::Rsa;
    use openssl::x509::X509NameBuilder;

    #[test]
    fn connector_enables_peer_verification_and_trust() {
        let connector = connector().unwrap();
        assert!(
            connector
                .context()
                .verify_mode()
                .contains(SslVerifyMode::PEER)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn loads_macos_system_root_certificates() {
        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        assert!(load_macos_keychain_roots(&mut builder).unwrap() > 0);
    }

    #[test]
    fn pem_loader_accepts_certificates_and_rejects_malformed_input() {
        let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
        let mut name = X509NameBuilder::new().unwrap();
        name.append_entry_by_text("CN", "synthetic.test").unwrap();
        let name = name.build();
        let mut certificate = X509::builder().unwrap();
        certificate.set_version(2).unwrap();
        certificate.set_subject_name(&name).unwrap();
        certificate.set_issuer_name(&name).unwrap();
        certificate.set_pubkey(&key).unwrap();
        certificate
            .set_not_before(&Asn1Time::days_from_now(0).unwrap())
            .unwrap();
        certificate
            .set_not_after(&Asn1Time::days_from_now(1).unwrap())
            .unwrap();
        certificate.sign(&key, MessageDigest::sha256()).unwrap();

        let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
        assert_eq!(
            add_pem_certificates(&mut builder, &certificate.build().to_pem().unwrap()).unwrap(),
            1
        );
        assert!(add_pem_certificates(&mut builder, b"not a certificate").is_err());
    }
}
