use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

const STUB_SCRIPT: &str = "#!/bin/sh\n\
if [ -n \"$STUB_LOG\" ]; then\n\
  echo \"$@\" >> \"$STUB_LOG\"\n\
fi\n\
if [ \"$1\" = \"--version\" ]; then\n\
  echo \"${STUB_VERSION:-2024.01.01}\"\n\
fi\n\
exit \"${STUB_EXIT:-0}\"\n";

fn temp_dir() -> PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-dl-cli-{}-{number}", std::process::id()));
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

#[test]
fn version_works_without_yt_dlp_installed() {
    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .env("PATH", "")
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"dl v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn version_flag_combined_with_other_operands_is_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .env("PATH", "")
        .args(["-v", "somefile.mp4"])
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
fn help_works_without_yt_dlp_installed() {
    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .env("PATH", "")
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: dl"));
}

#[test]
fn wrong_argument_count_prints_usage_without_yt_dlp() {
    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .env("PATH", "")
        .arg("only-one-arg")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: dl"));
}

#[test]
fn missing_yt_dlp_reports_install_instructions() {
    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .env("PATH", "")
        .args(["clip", "https://example.com/video"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("yt-dlp is not installed"));
    assert!(String::from_utf8_lossy(&output.stdout).contains("curl -L https://github.com/yt-dlp"));
}

#[cfg(unix)]
#[test]
fn downloads_with_extension_normalization_and_stub_success() {
    let directory = temp_dir();
    let bin_dir = directory.join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let log = directory.join("stub.log");
    write_stub(&bin_dir, "yt-dlp", STUB_SCRIPT);

    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .current_dir(&directory)
        .env("PATH", prepend_path(&bin_dir))
        .env("STUB_LOG", &log)
        .args(["clip.mkv", "https://example.com/video"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let logged = fs::read_to_string(&log).unwrap();
    assert!(logged.contains("clip.mkv.mp4"));
    assert!(logged.contains("--recode-video mp4"));
    assert!(logged.contains("https://example.com/video"));
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn existing_destination_is_refused() {
    let directory = temp_dir();
    let bin_dir = directory.join("bin");
    fs::create_dir(&bin_dir).unwrap();
    write_stub(&bin_dir, "yt-dlp", STUB_SCRIPT);
    fs::write(directory.join("clip.mp4"), b"existing").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .current_dir(&directory)
        .env("PATH", prepend_path(&bin_dir))
        .args(["clip.mp4", "https://example.com/video"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("File already exists: clip.mp4"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Error: file already exists"));
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn download_failure_reports_context_and_exit_code() {
    let directory = temp_dir();
    let bin_dir = directory.join("bin");
    fs::create_dir(&bin_dir).unwrap();
    write_stub(&bin_dir, "yt-dlp", STUB_SCRIPT);

    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .current_dir(&directory)
        .env("PATH", prepend_path(&bin_dir))
        .env("STUB_EXIT", "1")
        .args(["clip", "https://example.com/video"])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Error: download failed"));
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn update_flow_prints_versions_after_stub_upgrade() {
    let directory = temp_dir();
    let bin_dir = directory.join("bin");
    fs::create_dir(&bin_dir).unwrap();
    write_stub(&bin_dir, "yt-dlp", STUB_SCRIPT);

    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .env("PATH", prepend_path(&bin_dir))
        .env("STUB_VERSION", "2099.01.01")
        .arg("-u")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("dl v2.0.0"));
    assert!(text.contains("yt-dlp 2099.01.01"));
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn update_flow_reports_missing_yt_dlp() {
    let output = Command::new(env!("CARGO_BIN_EXE_dl"))
        .env("PATH", "")
        .arg("--update")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("yt-dlp is not installed"));
}

#[test]
fn documentation_describes_downloader_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## dl"));
    assert!(readme.contains("dl v2.0.0"));
    assert!(readme.contains("yt-dlp"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### dl"));
    assert!(arch.contains("yt-dlp presence check"));
}
