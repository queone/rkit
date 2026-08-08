use rkit::pman::{
    HttpRequest, HttpResponse, HttpTransport, TcpHttpTransport, TokenSource, run_with,
};
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

const AZM_STUB: &str = "#!/bin/sh\necho \"${STUB_TOKEN:-eyJstubtoken}\"\n";

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-pman-cli-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[cfg(unix)]
fn write_stub(dir: &Path, name: &str, script: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn prepend_path(dir: &Path) -> OsString {
    let mut paths = vec![dir.to_path_buf()];
    if let Some(existing) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(paths).unwrap()
}

// --- Subprocess (real binary, stub `azm` on PATH) coverage ---

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_pman"))
        .env("PATH", "")
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"pman v2.0.1\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_is_available_without_azm() {
    let output = Command::new(env!("CARGO_BIN_EXE_pman"))
        .env("PATH", "")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Azure REST API Caller"));
}

#[test]
fn version_flag_combined_with_other_operands_is_rejected_even_without_azm() {
    let output = Command::new(env!("CARGO_BIN_EXE_pman"))
        .env("PATH", "")
        .args(["-v", "https://graph.microsoft.com/v1.0/me"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("version flag cannot be combined with other operands")
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn missing_azm_reports_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_pman"))
        .env("PATH", "")
        .args(["GET", "https://graph.microsoft.com/v1.0/me"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Missing 'azm' binary!"));
}

#[cfg(unix)]
#[test]
fn bad_argument_count_prints_usage_when_azm_present() {
    let directory = temp_dir();
    write_stub(&directory, "azm", AZM_STUB);
    let output = Command::new(env!("CARGO_BIN_EXE_pman"))
        .env("PATH", prepend_path(&directory))
        .arg("GET")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage Examples"));
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn unsupported_endpoint_prints_usage_via_cli() {
    let directory = temp_dir();
    write_stub(&directory, "azm", AZM_STUB);
    let output = Command::new(env!("CARGO_BIN_EXE_pman"))
        .env("PATH", prepend_path(&directory))
        .args(["GET", "https://example.com/"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage Examples"));
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn stub_azm_token_flow_warns_on_invalid_token_and_stops_before_any_network_call() {
    let directory = temp_dir();
    write_stub(&directory, "azm", AZM_STUB);
    let output = Command::new(env!("CARGO_BIN_EXE_pman"))
        .env("PATH", prepend_path(&directory))
        .env("STUB_TOKEN", "not-a-jwt")
        .args([
            "GET",
            "https://management.azure.com/subscriptions?api-version=2022-04-01",
            "-d",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("WARNING: Token string is invalid!"));
    assert!(stderr.contains("Missing value for -d/--data"));
    fs::remove_dir_all(directory).unwrap();
}

// --- Library-seam coverage (no azm binary, no network) ---

struct FakeTokens {
    token: String,
    flags: Mutex<Vec<String>>,
}

impl TokenSource for FakeTokens {
    fn obtain(&self, flag: &str) -> Result<String, String> {
        self.flags.lock().unwrap().push(flag.to_owned());
        Ok(self.token.clone())
    }
}

struct FakeTransport {
    response: Mutex<Option<HttpResponse>>,
    fail: bool,
    captured: Mutex<Option<HttpRequest>>,
}

impl HttpTransport for FakeTransport {
    fn send(&self, request: &HttpRequest) -> io::Result<HttpResponse> {
        *self.captured.lock().unwrap() = Some(request.clone());
        if self.fail {
            return Err(io::Error::other("simulated transport failure"));
        }
        Ok(self.response.lock().unwrap().take().unwrap())
    }
}

fn fake_transport(status: u16, body: &[u8]) -> FakeTransport {
    FakeTransport {
        response: Mutex::new(Some(HttpResponse {
            status,
            body: body.to_vec(),
        })),
        fail: false,
        captured: Mutex::new(None),
    }
}

#[test]
fn version_flag_combined_with_other_operands_is_rejected_via_run_with() {
    let tokens = FakeTokens {
        token: String::new(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = fake_transport(200, b"");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        ["-v", "https://graph.microsoft.com/v1.0/me"],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 2);
    assert!(
        String::from_utf8_lossy(&stderr)
            .contains("version flag cannot be combined with other operands")
    );
    assert!(stdout.is_empty());
    assert!(tokens.flags.lock().unwrap().is_empty());
}

#[test]
fn graph_endpoint_selects_tmg_flag_and_uppercases_method() {
    let tokens = FakeTokens {
        token: "eyJhbGciOi".to_owned(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = fake_transport(200, b"ok");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        ["get", "https://graph.microsoft.com/v1.0/me"],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, b"ok");
    assert!(stderr.is_empty());
    assert_eq!(tokens.flags.lock().unwrap().as_slice(), ["-tmg"]);
    let captured = transport.captured.lock().unwrap().clone().unwrap();
    assert_eq!(captured.method, "GET");
    assert!(
        captured
            .headers
            .contains(&("Content-Type".to_owned(), "application/json".to_owned()))
    );
    assert!(
        captured
            .headers
            .contains(&("Authorization".to_owned(), "Bearer eyJhbGciOi".to_owned()))
    );
}

#[test]
fn management_endpoint_selects_taz_flag() {
    let tokens = FakeTokens {
        token: "eyJtoken".to_owned(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = fake_transport(200, b"ok");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        [
            "GET",
            "https://management.azure.com/subscriptions?api-version=2022-04-01",
        ],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert_eq!(tokens.flags.lock().unwrap().as_slice(), ["-taz"]);
}

#[test]
fn unsupported_endpoint_prints_usage() {
    let tokens = FakeTokens {
        token: String::new(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = fake_transport(200, b"");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        ["GET", "https://example.com/"],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 1);
    assert!(String::from_utf8_lossy(&stdout).contains("Usage Examples"));
    assert!(tokens.flags.lock().unwrap().is_empty());
}

#[test]
fn invalid_token_warns_but_still_sends_request() {
    let tokens = FakeTokens {
        token: "not-a-jwt".to_owned(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = fake_transport(200, b"ok");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        ["GET", "https://graph.microsoft.com/v1.0/me"],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, b"ok");
    assert!(String::from_utf8_lossy(&stderr).contains("WARNING: Token string is invalid!"));
}

#[test]
fn valid_token_produces_no_warning() {
    let tokens = FakeTokens {
        token: "eyJvalid".to_owned(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = fake_transport(200, b"ok");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        ["GET", "https://graph.microsoft.com/v1.0/me"],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
}

#[test]
fn data_flag_sets_request_body() {
    let tokens = FakeTokens {
        token: "eyJtoken".to_owned(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = fake_transport(200, b"");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        [
            "POST",
            "https://management.azure.com/subscriptions",
            "-d",
            "{\"a\":1}",
        ],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert!(stderr.is_empty());
    let captured = transport.captured.lock().unwrap().clone().unwrap();
    assert_eq!(captured.body.as_deref(), Some(b"{\"a\":1}".as_slice()));
}

#[test]
fn missing_data_value_reports_error() {
    let tokens = FakeTokens {
        token: "eyJtoken".to_owned(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = fake_transport(200, b"");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        ["GET", "https://graph.microsoft.com/v1.0/me", "-d"],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 1);
    assert!(String::from_utf8_lossy(&stderr).contains("Missing value for -d/--data"));
}

#[test]
fn non_2xx_status_still_exits_zero_and_prints_body() {
    let tokens = FakeTokens {
        token: "eyJtoken".to_owned(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = fake_transport(404, b"missing");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        ["GET", "https://graph.microsoft.com/v1.0/nope"],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, b"missing");
}

#[test]
fn transport_failure_reports_error_and_exits_nonzero() {
    let tokens = FakeTokens {
        token: "eyJtoken".to_owned(),
        flags: Mutex::new(Vec::new()),
    };
    let transport = FakeTransport {
        response: Mutex::new(None),
        fail: true,
        captured: Mutex::new(None),
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = run_with(
        ["GET", "https://graph.microsoft.com/v1.0/nope"],
        "2.0.0",
        &tokens,
        &transport,
        &mut stdout,
        &mut stderr,
    );
    assert_eq!(code, 1);
    assert!(stdout.is_empty());
    assert!(String::from_utf8_lossy(&stderr).contains("HTTP request failed"));
}

// --- Real HTTP/1.1 client against a local TCP server ---

/// Reads a full HTTP request (headers plus any declared body) from `stream`,
/// looping until the terminating blank line and the full `Content-Length`
/// body have arrived; a single `read` call is not guaranteed to receive the
/// whole request in one syscall.
fn read_full_request(stream: &mut std::net::TcpStream) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let read = stream.read(&mut chunk).unwrap();
        assert_ne!(read, 0, "connection closed before a full request arrived");
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(header_end) = find_double_crlf(&buffer) {
            let headers_text = String::from_utf8_lossy(&buffer[..header_end]);
            let content_length = headers_text
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.trim()
                            .eq_ignore_ascii_case("content-length")
                            .then(|| value.trim())
                    })
                })
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            let body_so_far = buffer.len() - (header_end + 4);
            if body_so_far >= content_length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

fn find_double_crlf(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

#[test]
fn tcp_transport_sends_headers_and_body_and_parses_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request_text = read_full_request(&mut stream);
        let response_body = b"pong";
        let response = format!(
            "HTTP/1.1 201 Created\r\nContent-Length: {}\r\n\r\n",
            response_body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
        stream.write_all(response_body).unwrap();
        request_text
    });

    let request = HttpRequest {
        method: "POST".to_owned(),
        url: format!("http://{addr}/widgets?x=1"),
        headers: vec![("Authorization".to_owned(), "Bearer test".to_owned())],
        body: Some(b"{\"a\":1}".to_vec()),
    };

    let response = TcpHttpTransport.send(&request).unwrap();
    let request_text = handle.join().unwrap();

    assert_eq!(response.status, 201);
    assert_eq!(response.body, b"pong");
    assert!(request_text.starts_with("POST /widgets?x=1 HTTP/1.1\r\n"));
    assert!(request_text.contains("Authorization: Bearer test\r\n"));
    assert!(request_text.contains("Content-Length: 7\r\n"));
    assert!(request_text.ends_with("{\"a\":1}"));
}

#[test]
fn tcp_transport_decodes_chunked_response_bodies() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut buffer = [0u8; 4096];
        let _ = stream.read(&mut buffer).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\npong\r\n0\r\n\r\n",
            )
            .unwrap();
    });
    let request = HttpRequest {
        method: "GET".to_owned(),
        url: format!("http://{addr}/"),
        headers: vec![],
        body: None,
    };
    let response = TcpHttpTransport.send(&request).unwrap();
    handle.join().unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"pong");
}

#[test]
fn documentation_describes_azure_caller_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## pman"));
    assert!(readme.contains("pman v2.0.1"));
    assert!(readme.contains("azm"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### pman"));
    assert!(arch.contains("azm"));
}
