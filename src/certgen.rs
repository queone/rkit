//! Self-signed certificate, private-key, and CSR generation.

use openssl::asn1::{Asn1Integer, Asn1Time};
use openssl::bn::{BigNum, MsbOption};
use openssl::hash::MessageDigest;
use openssl::pkey::{PKey, Private};
use openssl::rsa::Rsa;
use openssl::stack::Stack;
use openssl::x509::extension::{
    BasicConstraints, ExtendedKeyUsage, KeyUsage, SubjectAlternativeName,
};
use openssl::x509::{X509, X509NameBuilder, X509Req};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, Write};
use std::path::Path;

const PROGRAM_NAME: &str = "certgen";

/// Run certgen with injectable streams for deterministic tests.
pub fn run<I, S, R, W, E>(
    args: I,
    version: &str,
    input: &mut R,
    stdout: &mut W,
    stderr: &mut E,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    R: BufRead,
    W: Write,
    E: Write,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "-v" | "--version") {
        let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
        return 0;
    }
    if args.len() != 1 || args[0].trim().is_empty() {
        usage(stdout);
        return 1;
    }

    let fqdn = args[0].trim();
    let subject = match build_subject(fqdn) {
        Ok(subject) => subject,
        Err(error) => return fail(stderr, "build certificate subject", error),
    };
    let _ = writeln!(
        stdout,
        "\nCOUNTRY=US   STATE=NY   LOC=New York   ORG=My Org   UNIT=My Unit   DOMAIN={}\n",
        fqdn
    );
    let _ = write!(
        stdout,
        "Proceed to create a self-signed cert + key, with a CSR for domain '{fqdn}'? Y/N "
    );
    if let Err(error) = stdout.flush() {
        return fail(stderr, "flush confirmation prompt", error.to_string());
    }
    let mut response = String::new();
    if input.read_line(&mut response).is_err() {
        return fail(
            stderr,
            "read confirmation",
            "standard input could not be read".to_string(),
        );
    }
    if !matches!(response.trim(), "Y" | "y") {
        let _ = writeln!(stdout, "\nAborted.");
        return 1;
    }

    let _ = writeln!(
        stdout,
        "\nGenerating private key, self-signed cert, and CSR ..."
    );
    let rsa = match Rsa::generate(2048) {
        Ok(rsa) => rsa,
        Err(error) => return fail(stderr, "generate RSA key", error.to_string()),
    };
    let key = match PKey::from_rsa(rsa.clone()) {
        Ok(key) => key,
        Err(error) => return fail(stderr, "load RSA key", error.to_string()),
    };
    let private_key_pem = match rsa.private_key_to_pem() {
        Ok(pem) => pem,
        Err(error) => return fail(stderr, "encode private key", error.to_string()),
    };
    if let Err(error) = write_pem(
        &format!("{fqdn}.key"),
        "RSA PRIVATE KEY",
        &private_key_pem,
        0o600,
    ) {
        return fail(stderr, "write private key", error);
    }

    let csr = match build_csr(&subject, &key, fqdn) {
        Ok(csr) => csr,
        Err(error) => return fail(stderr, "create CSR", error),
    };
    let csr_pem = match csr.to_pem() {
        Ok(pem) => pem,
        Err(error) => return fail(stderr, "encode CSR", error.to_string()),
    };
    if let Err(error) = write_pem(
        &format!("{fqdn}.csr"),
        "CERTIFICATE REQUEST",
        &csr_pem,
        0o644,
    ) {
        return fail(stderr, "write CSR", error);
    }

    let certificate = match build_certificate(&subject, &key, fqdn) {
        Ok(certificate) => certificate,
        Err(error) => return fail(stderr, "create certificate", error),
    };
    let certificate_pem = match certificate.to_pem() {
        Ok(pem) => pem,
        Err(error) => return fail(stderr, "encode certificate", error.to_string()),
    };
    if let Err(error) = write_pem(
        &format!("{fqdn}.crt"),
        "CERTIFICATE",
        &certificate_pem,
        0o644,
    ) {
        return fail(stderr, "write certificate", error);
    }

    let _ = writeln!(
        stdout,
        "\n1) You may use the self-signed cert + private key"
    );
    let _ = writeln!(
        stdout,
        "2) Or submit the CSR to a public CA (Entrust, etc.)"
    );
    list_files(fqdn, stdout);
    0
}

fn usage(stdout: &mut impl Write) {
    let _ = writeln!(stdout, "Usage: {PROGRAM_NAME} <common-name>");
}

fn fail(stderr: &mut impl Write, operation: &str, error: String) -> u8 {
    let _ = writeln!(stderr, "{PROGRAM_NAME}: {operation}: {error}");
    1
}

fn build_subject(fqdn: &str) -> Result<openssl::x509::X509Name, String> {
    let mut builder = X509NameBuilder::new().map_err(|error| error.to_string())?;
    for (key, value) in [
        ("C", "US"),
        ("ST", "NY"),
        ("L", "New York"),
        ("O", "My Org"),
        ("OU", "My Unit"),
        ("CN", fqdn),
    ] {
        builder
            .append_entry_by_text(key, value)
            .map_err(|error| error.to_string())?;
    }
    Ok(builder.build())
}

fn build_csr(
    subject: &openssl::x509::X509NameRef,
    key: &PKey<Private>,
    fqdn: &str,
) -> Result<X509Req, String> {
    let mut builder = X509Req::builder().map_err(|error| error.to_string())?;
    builder
        .set_subject_name(subject)
        .map_err(|error| error.to_string())?;
    builder.set_pubkey(key).map_err(|error| error.to_string())?;
    let mut extensions = Stack::new().map_err(|error| error.to_string())?;
    let context = builder.x509v3_context(None);
    let extension = SubjectAlternativeName::new()
        .dns(fqdn)
        .build(&context)
        .map_err(|error| error.to_string())?;
    extensions
        .push(extension)
        .map_err(|error| error.to_string())?;
    builder
        .add_extensions(&extensions)
        .map_err(|error| error.to_string())?;
    builder
        .sign(key, MessageDigest::sha256())
        .map_err(|error| error.to_string())?;
    Ok(builder.build())
}

fn build_certificate(
    subject: &openssl::x509::X509NameRef,
    key: &PKey<Private>,
    fqdn: &str,
) -> Result<X509, String> {
    let mut serial = BigNum::new().map_err(|error| error.to_string())?;
    serial
        .pseudo_rand(8, MsbOption::MAYBE_ZERO, false)
        .map_err(|error| error.to_string())?;
    let serial = Asn1Integer::from_bn(&serial).map_err(|error| error.to_string())?;
    let not_before = Asn1Time::days_from_now(0).map_err(|error| error.to_string())?;
    let not_after = Asn1Time::days_from_now(3650).map_err(|error| error.to_string())?;
    let mut builder = X509::builder().map_err(|error| error.to_string())?;
    builder.set_version(2).map_err(|error| error.to_string())?;
    builder
        .set_serial_number(&serial)
        .map_err(|error| error.to_string())?;
    builder
        .set_subject_name(subject)
        .map_err(|error| error.to_string())?;
    builder
        .set_issuer_name(subject)
        .map_err(|error| error.to_string())?;
    builder.set_pubkey(key).map_err(|error| error.to_string())?;
    builder
        .set_not_before(&not_before)
        .map_err(|error| error.to_string())?;
    builder
        .set_not_after(&not_after)
        .map_err(|error| error.to_string())?;
    let constraints = BasicConstraints::new()
        .critical()
        .build()
        .map_err(|error| error.to_string())?;
    builder
        .append_extension(constraints)
        .map_err(|error| error.to_string())?;
    let key_usage = KeyUsage::new()
        .digital_signature()
        .key_encipherment()
        .build()
        .map_err(|error| error.to_string())?;
    builder
        .append_extension(key_usage)
        .map_err(|error| error.to_string())?;
    let extended_key_usage = ExtendedKeyUsage::new()
        .server_auth()
        .build()
        .map_err(|error| error.to_string())?;
    builder
        .append_extension(extended_key_usage)
        .map_err(|error| error.to_string())?;
    let context = builder.x509v3_context(None, None);
    let san = SubjectAlternativeName::new()
        .dns(fqdn)
        .build(&context)
        .map_err(|error| error.to_string())?;
    builder
        .append_extension(san)
        .map_err(|error| error.to_string())?;
    builder
        .sign(key, MessageDigest::sha256())
        .map_err(|error| error.to_string())?;
    Ok(builder.build())
}

fn write_pem(path: &str, label: &str, bytes: &[u8], mode: u32) -> Result<(), String> {
    let existed = Path::new(path).exists();
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.write_all(bytes).map_err(|error| error.to_string())?;
    file.flush().map_err(|error| error.to_string())?;
    if !existed {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode))
                .map_err(|error| error.to_string())?;
        }
    }
    if bytes.is_empty() {
        return Err(format!("{label} encoding produced no bytes"));
    }
    Ok(())
}

fn list_files(prefix: &str, stdout: &mut impl Write) {
    let Ok(entries) = fs::read_dir(".") else {
        return;
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&format!("{prefix}."))
            && let Ok(metadata) = entry.metadata()
        {
            names.push((name, metadata.len()));
        }
    }
    names.sort_by(|left, right| left.0.cmp(&right.0));
    for (name, size) in names {
        let _ = writeln!(stdout, "{name}\t{size} bytes");
    }
}

#[cfg(test)]
mod tests {
    use super::run;
    use std::io::{Cursor, Write};

    struct FlushWriter {
        bytes: Vec<u8>,
        flushed_lengths: Vec<usize>,
    }

    impl Write for FlushWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushed_lengths.push(self.bytes.len());
            Ok(())
        }
    }

    #[test]
    fn flushes_confirmation_prompt_before_reading_input() {
        let mut input = Cursor::new(b"N\n".to_vec());
        let mut stdout = FlushWriter {
            bytes: Vec::new(),
            flushed_lengths: Vec::new(),
        };
        let mut stderr = Vec::new();
        let code = run(
            ["example.test"],
            "2.0.0",
            &mut input,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 1);
        assert!(stderr.is_empty());
        assert_eq!(stdout.flushed_lengths.len(), 1);
        let flushed = stdout.flushed_lengths[0];
        assert!(stdout.bytes[..flushed].ends_with(
            b"Proceed to create a self-signed cert + key, with a CSR for domain 'example.test'? Y/N "
        ));
    }
}
