//! Azure REST API calling behavior for the `pman` utility.

use openssl::ssl::{SslConnector, SslMethod};
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::Command;

const PROGRAM_NAME: &str = "pman";

/// An HTTP request to send through an [`HttpTransport`].
#[derive(Clone)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// The status and body returned by an [`HttpTransport`].
#[derive(Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

/// Sends an [`HttpRequest`] and returns its [`HttpResponse`]; injectable so
/// tests can exercise `run_with` without any network access.
pub trait HttpTransport {
    fn send(&self, request: &HttpRequest) -> io::Result<HttpResponse>;
}

/// Obtains an Azure access token for a `azm` token flag (`-tmg` or `-taz`);
/// injectable so tests can exercise `run_with` without the `azm` binary.
pub trait TokenSource {
    fn obtain(&self, flag: &str) -> Result<String, String>;
}

/// Minimal HTTP/1.1 client over std TCP, with OpenSSL TLS for `https`.
pub struct TcpHttpTransport;

impl HttpTransport for TcpHttpTransport {
    fn send(&self, request: &HttpRequest) -> io::Result<HttpResponse> {
        let parsed = parse_url(&request.url).map_err(io::Error::other)?;
        let mut header_bytes = Vec::new();
        write!(
            header_bytes,
            "{} {} HTTP/1.1\r\n",
            request.method, parsed.path
        )?;
        write!(header_bytes, "Host: {}\r\n", parsed.host)?;
        for (name, value) in &request.headers {
            write!(header_bytes, "{name}: {value}\r\n")?;
        }
        if let Some(body) = &request.body {
            write!(header_bytes, "Content-Length: {}\r\n", body.len())?;
        }
        write!(header_bytes, "Connection: close\r\n\r\n")?;

        let address = format!("{}:{}", parsed.host, parsed.port);
        let stream = TcpStream::connect(&address)?;

        if parsed.scheme == "https" {
            let connector = SslConnector::builder(SslMethod::tls())
                .map_err(|error| io::Error::other(error.to_string()))?
                .build();
            let mut tls = connector
                .connect(&parsed.host, stream)
                .map_err(|error| io::Error::other(error.to_string()))?;
            tls.write_all(&header_bytes)?;
            if let Some(body) = &request.body {
                tls.write_all(body)?;
            }
            read_response(&mut tls)
        } else {
            let mut stream = stream;
            stream.write_all(&header_bytes)?;
            if let Some(body) = &request.body {
                stream.write_all(body)?;
            }
            read_response(&mut stream)
        }
    }
}

/// Runs `azm -<flag>` and returns its trimmed stdout as the access token.
pub struct AzmTokenSource;

impl TokenSource for AzmTokenSource {
    fn obtain(&self, flag: &str) -> Result<String, String> {
        let output = Command::new("azm")
            .arg(flag)
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!("exit status {}", output.status));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
}

/// Runs `pman` and writes its process output to the supplied streams.
///
/// `-v`/`--version` and help are handled before the `azm` presence check,
/// matching Go's order. Adds `-?`/`-h`/`--help`, which Go had no equivalent
/// for.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let is_short_circuit =
        args.len() == 1 && (is_flag(&args[0], "-v", "--version") || is_help_flag(&args[0]));
    let requests_version = args.iter().any(|arg| is_flag(arg, "-v", "--version"));
    if !is_short_circuit && !requests_version && !azm_available() {
        let _ = writeln!(stderr, "Missing 'azm' binary!");
        return 1;
    }
    run_with(
        args,
        version,
        &AzmTokenSource,
        &TcpHttpTransport,
        stdout,
        stderr,
    )
}

/// Runs `pman` against an injectable token source and HTTP transport,
/// independent of the `azm` binary and the network.
pub fn run_with<I, S, T, H, W, E>(
    args: I,
    version: &str,
    tokens: &T,
    transport: &H,
    stdout: &mut W,
    stderr: &mut E,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    T: TokenSource,
    H: HttpTransport,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();

    if args.len() != 1 && args.iter().any(|arg| is_flag(arg, "-v", "--version")) {
        let _ = writeln!(
            stderr,
            "{PROGRAM_NAME}: version flag cannot be combined with other operands"
        );
        return 2;
    }

    if args.len() == 1 {
        if is_flag(&args[0], "-v", "--version") {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            return 0;
        }
        if is_help_flag(&args[0]) {
            let _ = write!(stdout, "{}", usage_text(version));
            return 0;
        }
    }

    if args.len() < 2 {
        let _ = write!(stdout, "{}", usage_text(version));
        return 1;
    }

    let method = args[0].to_string_lossy().to_uppercase();
    let url = args[1].to_string_lossy().into_owned();

    let flag = match token_flag_for_url(&url) {
        Some(flag) => flag,
        None => {
            let _ = write!(stdout, "{}", usage_text(version));
            return 1;
        }
    };

    let token = match tokens.obtain(flag) {
        Ok(token) => token,
        Err(error) => {
            let _ = writeln!(stderr, "Failed to obtain token: {error}");
            return 1;
        }
    };

    if !token.starts_with("eyJ") {
        let _ = writeln!(stderr);
        let _ = writeln!(stderr, "WARNING: Token string is invalid!");
        let _ = writeln!(stderr);
    }

    let headers = vec![
        ("Content-Type".to_owned(), "application/json".to_owned()),
        ("Authorization".to_owned(), format!("Bearer {token}")),
    ];

    let extra = &args[2..];
    let mut body = None;
    let mut index = 0;
    while index < extra.len() {
        let value = extra[index].to_string_lossy();
        if value == "-d" || value == "--data" {
            if index + 1 >= extra.len() {
                let _ = writeln!(stderr, "Missing value for -d/--data");
                return 1;
            }
            body = Some(extra[index + 1].to_string_lossy().into_owned().into_bytes());
            index += 1;
        }
        index += 1;
    }

    let request = HttpRequest {
        method,
        url,
        headers,
        body,
    };
    match transport.send(&request) {
        Ok(response) => {
            let _ = stdout.write_all(&response.body);
            0
        }
        Err(error) => {
            let _ = writeln!(stderr, "HTTP request failed: {error}");
            1
        }
    }
}

fn token_flag_for_url(url: &str) -> Option<&'static str> {
    if url.contains("https://graph.microsoft.com") {
        Some("-tmg")
    } else if url.contains("https://management.azure.com") {
        Some("-taz")
    } else {
        None
    }
}

fn azm_available() -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| is_executable(&dir.join("azm")))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}

fn is_help_flag(value: &OsString) -> bool {
    value == "-?" || value == "-h" || value == "--help"
}

fn usage_text(version: &str) -> String {
    format!(
        "{PROGRAM_NAME} Azure REST API Caller v{version}\n\
  This utility relies on the 'azm' command-line utility being installed,\n\
  authenticated, and properly configured to obtain Azure access tokens.\n\
  It uses azm to retrieve Microsoft Graph or Azure Resource Manager tokens\n\
  based on the target endpoint.\n\
\n\
  Usage Examples:\n\
    {PROGRAM_NAME} GET \"https://graph.microsoft.com/v1.0/me\"\n\
    {PROGRAM_NAME} GET \"https://management.azure.com/subscriptions?api-version=2022-04-01\"\n\
    {PROGRAM_NAME} GET \"https://graph.microsoft.com/v1.0/applications/<id>\"\n\
\n\
  Options:\n\
    -v, --version      Print version and exit\n\
    -?, -h, --help     Print this usage page\n"
    )
}

struct ParsedUrl {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Result<ParsedUrl, String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| "missing scheme".to_owned())?;
    let scheme = scheme.to_lowercase();
    let default_port = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => return Err(format!("unsupported scheme: {scheme}")),
    };
    let (authority, path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, "/"),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port_text))
            if !port_text.is_empty() && port_text.bytes().all(|byte| byte.is_ascii_digit()) =>
        {
            let port = port_text
                .parse::<u16>()
                .map_err(|error| error.to_string())?;
            (host.to_owned(), port)
        }
        _ => (authority.to_owned(), default_port),
    };
    Ok(ParsedUrl {
        scheme,
        host,
        port,
        path: path.to_owned(),
    })
}

fn read_response<R: Read>(reader: &mut R) -> io::Result<HttpResponse> {
    let mut buffered = BufReader::new(reader);
    let mut status_line = String::new();
    buffered.read_line(&mut status_line)?;
    let status = parse_status_code(&status_line)?;

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        let read = buffered.read_line(&mut line)?;
        if read == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim();
            if name == "content-length" {
                content_length = value.parse::<usize>().ok();
            } else if name == "transfer-encoding" && value.eq_ignore_ascii_case("chunked") {
                chunked = true;
            }
        }
    }

    let body = if chunked {
        read_chunked_body(&mut buffered)?
    } else if let Some(length) = content_length {
        let mut body = vec![0u8; length];
        buffered.read_exact(&mut body)?;
        body
    } else {
        let mut body = Vec::new();
        buffered.read_to_end(&mut body)?;
        body
    };

    Ok(HttpResponse { status, body })
}

fn parse_status_code(line: &str) -> io::Result<u16> {
    line.split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| io::Error::other(format!("malformed status line: {line:?}")))
}

fn read_chunked_body<R: BufRead>(reader: &mut R) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let mut size_line = String::new();
        reader.read_line(&mut size_line)?;
        let size_text = size_line.trim().split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|error| io::Error::other(format!("malformed chunk size: {error}")))?;
        if size == 0 {
            let mut trailer = String::new();
            reader.read_line(&mut trailer)?;
            break;
        }
        let mut chunk = vec![0u8; size];
        reader.read_exact(&mut chunk)?;
        body.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf)?;
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_flag_routes_by_url_substring() {
        assert_eq!(
            token_flag_for_url("https://graph.microsoft.com/v1.0/me"),
            Some("-tmg")
        );
        assert_eq!(
            token_flag_for_url("https://management.azure.com/subscriptions"),
            Some("-taz")
        );
        assert_eq!(token_flag_for_url("https://example.com/"), None);
    }

    #[test]
    fn url_parsing_defaults_ports_and_splits_path() {
        let parsed = parse_url("https://graph.microsoft.com/v1.0/me?x=1").unwrap();
        assert_eq!(parsed.scheme, "https");
        assert_eq!(parsed.host, "graph.microsoft.com");
        assert_eq!(parsed.port, 443);
        assert_eq!(parsed.path, "/v1.0/me?x=1");

        let parsed = parse_url("http://127.0.0.1:8080/widgets").unwrap();
        assert_eq!(parsed.host, "127.0.0.1");
        assert_eq!(parsed.port, 8080);
        assert_eq!(parsed.path, "/widgets");

        let parsed = parse_url("http://example.com").unwrap();
        assert_eq!(parsed.path, "/");
    }
}
