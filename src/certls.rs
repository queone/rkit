//! Verified TLS certificate inspection.

use openssl::asn1::Asn1TimeRef;
use openssl::ssl::{SslConnector, SslConnectorBuilder, SslMethod, SslVerifyMode};
use openssl::x509::X509;
use std::io::Write;
use std::net::TcpStream;
#[cfg(target_os = "macos")]
use std::process::Command;

const PROGRAM_NAME: &str = "certls";

/// Run certls using the platform's trusted certificate paths.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    W: Write,
    E: Write,
{
    let mut builder = match SslConnector::builder(SslMethod::tls()) {
        Ok(builder) => builder,
        Err(error) => return fail(stderr, "create TLS connector", error.to_string()),
    };
    builder.set_verify(SslVerifyMode::PEER);
    if let Err(error) = configure_trust(&mut builder) {
        return fail(stderr, "load trusted certificate paths", error);
    }
    let connector = builder.build();
    run_with_connector(args, version, &connector, stdout, stderr)
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
                "default paths: {}; macOS keychain: {}",
                default_error.unwrap_or_default(),
                keychain_error.unwrap_or_default()
            ));
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    default_error.map_or(Ok(()), Err)
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
            last_error = Some(String::from_utf8_lossy(&output.stderr).trim().to_string());
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
    let certificates = X509::stack_from_pem(pem).map_err(|error| error.to_string())?;
    let mut loaded = 0;
    for certificate in certificates {
        if builder.cert_store_mut().add_cert(certificate).is_ok() {
            loaded += 1;
        }
    }
    Ok(loaded)
}

/// Run certls with an injected connector for deterministic tests.
pub fn run_with_connector<I, S, W, E>(
    args: I,
    version: &str,
    connector: &SslConnector,
    stdout: &mut W,
    stderr: &mut E,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
    W: Write,
    E: Write,
{
    let args: Vec<String> = args.into_iter().map(Into::into).collect();
    if args.len() == 1 && matches!(args[0].as_str(), "-v" | "--version") {
        let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
        return 0;
    }
    if args.len() != 1 {
        usage(stdout);
        return 1;
    }
    let Some((host, port)) = parse_target(&args[0]) else {
        usage(stdout);
        return 1;
    };
    if host.is_empty() {
        usage(stdout);
        return 1;
    }
    let address = format!("{host}:{port}");
    let stream = match TcpStream::connect(&address) {
        Ok(stream) => stream,
        Err(error) => return fail(stderr, "TLS connection", error.to_string()),
    };
    let stream = match connector.connect(&host, stream) {
        Ok(stream) => stream,
        Err(error) => return fail(stderr, "TLS connection", error.to_string()),
    };
    let Some(certificate) = stream.ssl().peer_certificate() else {
        return fail(
            stderr,
            "inspect certificate",
            "No certificates presented by server".to_string(),
        );
    };
    print_certificate(
        &certificate,
        &host,
        &port,
        stream.ssl().version_str(),
        stdout,
    );
    0
}

fn parse_target(target: &str) -> Option<(String, String)> {
    if target.matches(':').count() > 1 {
        return None;
    }
    if let Some((host, port)) = target.split_once(':') {
        if host.is_empty() || port.is_empty() {
            return None;
        }
        return Some((host.to_string(), port.to_string()));
    }
    Some((target.to_string(), "443".to_string()))
}

fn usage(stdout: &mut impl Write) {
    let _ = writeln!(stdout, "Print SSL certificate details for given FQDN:Port.");
    let _ = writeln!(stdout);
    let _ = writeln!(stdout, "Usage: {PROGRAM_NAME} FQDN[:PORT]");
    let _ = writeln!(stdout, "  Examples:");
    let _ = writeln!(
        stdout,
        "    {PROGRAM_NAME} microsoft.com     Uses 443 by default"
    );
    let _ = writeln!(
        stdout,
        "    {PROGRAM_NAME} mysite.com:1473   Uses port 1473"
    );
}

fn fail(stderr: &mut impl Write, operation: &str, error: String) -> u8 {
    let _ = writeln!(stderr, "{PROGRAM_NAME}: {operation}: {error}");
    1
}

fn print_certificate(
    certificate: &X509,
    host: &str,
    port: &str,
    tls_version: &str,
    stdout: &mut impl Write,
) {
    let _ = writeln!(stdout, "==> TLS version {tls_version}");
    let _ = writeln!(stdout, "==> FQDN:Port {host}:{port}");
    let _ = writeln!(
        stdout,
        "==> EXPIRY: NotBefore={} NotAfter={}",
        format_time(certificate.not_before()),
        format_time(certificate.not_after())
    );
    let _ = writeln!(stdout, "==> LIST");
    if let Some(names) = certificate.subject_alt_names() {
        for name in names {
            if let Some(dns) = name.dnsname() {
                let _ = writeln!(stdout, "{dns}");
            }
        }
    }
}

fn format_time(value: &Asn1TimeRef) -> String {
    let raw = value.to_string();
    let fields: Vec<&str> = raw.split_whitespace().collect();
    if fields.len() == 5 && fields[4] == "GMT" {
        let month = match fields[0] {
            "Jan" => "01",
            "Feb" => "02",
            "Mar" => "03",
            "Apr" => "04",
            "May" => "05",
            "Jun" => "06",
            "Jul" => "07",
            "Aug" => "08",
            "Sep" => "09",
            "Oct" => "10",
            "Nov" => "11",
            "Dec" => "12",
            _ => return value.to_string(),
        };
        return format!("{}-{month}-{:0>2}T{}Z", fields[3], fields[1], fields[2]);
    }
    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::parse_target;

    #[cfg(target_os = "macos")]
    #[test]
    fn loads_macos_system_root_certificates() {
        let mut builder =
            openssl::ssl::SslConnector::builder(openssl::ssl::SslMethod::tls()).unwrap();
        let loaded = super::load_macos_keychain_roots(&mut builder).unwrap();
        assert!(loaded > 0);
    }

    #[test]
    fn target_parser_defaults_port_and_rejects_malformed_shapes() {
        assert_eq!(
            parse_target("example.test"),
            Some(("example.test".to_string(), "443".to_string()))
        );
        assert_eq!(
            parse_target("example.test:8443"),
            Some(("example.test".to_string(), "8443".to_string()))
        );
        assert_eq!(parse_target("example.test:8443:extra"), None);
        assert_eq!(parse_target("example.test:"), None);
    }
}
