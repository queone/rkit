use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_home() -> PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-sms-cli-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

fn sms_command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_sms"));
    command.env("HOME", home);
    command.env_remove("XDG_CONFIG_HOME");
    command
}

fn write_config(home: &Path, svcurl: &str, svckey: &str) {
    let dir = home.join(".config").join("sms");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("config.ini"),
        format!("[global]\nsvcurl = {svcurl}\nsvckey = {svckey}\n"),
    )
    .unwrap();
}

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
fn version_is_terminal_and_exact() {
    let home = temp_home();
    let output = sms_command(&home).arg("--version").output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"sms v2.0.0\n");
    assert!(output.stderr.is_empty());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn bad_argument_count_prints_usage_and_exits_zero() {
    let home = temp_home();
    let output = sms_command(&home).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("SMS CLI utility"));

    let output = sms_command(&home).args(["a", "b", "c"]).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("SMS CLI utility"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn version_combined_with_operand_is_rejected() {
    let home = temp_home();
    let output = sms_command(&home)
        .args(["-v", "5551234567"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("version flag cannot be combined with other operands")
    );
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn skeleton_config_creation_then_reports_already_exists() {
    let home = temp_home();
    let created = sms_command(&home).arg("-y").output().unwrap();
    assert!(created.status.success());
    assert!(created.stdout.is_empty());
    let cfg = home.join(".config").join("sms").join("config.ini");
    assert!(cfg.exists());
    assert!(fs::read_to_string(&cfg).unwrap().contains("svcurl"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&cfg).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    let again = sms_command(&home).arg("--init").output().unwrap();
    assert!(again.status.success());
    assert!(String::from_utf8_lossy(&again.stdout).contains("already a"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn missing_config_reports_error_on_stdout_and_exits_one() {
    let home = temp_home();
    let output = sms_command(&home)
        .args(["5551234567", "hello"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Missing"));
    assert!(output.stderr.is_empty());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn legacy_config_migrates_before_send_and_reports_over_stderr() {
    let home = temp_home();
    fs::write(
        home.join(".smsrc"),
        "[global]\nsvcurl = http://127.0.0.1:1/unreachable\nsvckey = legacykey\n",
    )
    .unwrap();
    let output = sms_command(&home)
        .args(["5551234567", "hello"])
        .output()
        .unwrap();
    assert!(!home.join(".smsrc").exists());
    assert!(home.join(".config").join("sms").join("config.ini").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("migrated"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn successful_send_posts_sorted_form_fields_and_prints_summary() {
    let home = temp_home();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    write_config(&home, &format!("http://{addr}/text"), "testkey");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request_text = read_full_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
        request_text
    });

    let output = sms_command(&home)
        .args(["5551234567", "hello there"])
        .output()
        .unwrap();
    let request_text = handle.join().unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("testkey  5551234567  hello there"));
    assert!(request_text.starts_with("POST /text HTTP/1.1\r\n"));
    assert!(request_text.contains("Content-Type: application/x-www-form-urlencoded\r\n"));
    assert!(request_text.ends_with("key=testkey&message=hello+there&phone=5551234567"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn non_200_status_reports_error_on_stdout_and_exits_one() {
    let home = temp_home();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    write_config(&home, &format!("http://{addr}/text"), "testkey");

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let _ = read_full_request(&mut stream);
        stream
            .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
            .unwrap();
    });

    let output = sms_command(&home)
        .args(["5551234567", "hello"])
        .output()
        .unwrap();
    handle.join().unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Error. HTTP error code = 400"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn unreachable_svcurl_reports_error_and_exits_one() {
    let home = temp_home();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener); // free the port so the connection is refused

    write_config(&home, &format!("http://127.0.0.1:{port}/text"), "testkey");
    let output = sms_command(&home)
        .args(["5551234567", "hello"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("request failed"));
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn documentation_describes_sms_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## sms"));
    assert!(readme.contains("sms v2.0.0"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### sms"));
}
