use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> std::path::PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-rnlower-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_rnlower"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"rnlower v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn prompt_abort_returns_one_without_mutation() {
    let directory = temp_dir();
    fs::write(directory.join("MIXED.TXT"), b"x").unwrap();
    let output = run_with_input(&directory, b"N\n");
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("Rename ALL filenames in CWD to lowercase? Y/N ")
    );
    assert!(directory.join("MIXED.TXT").exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acceptance_lowercases_sorted_entries_and_includes_directories() {
    let directory = temp_dir();
    fs::write(directory.join("ZED.TXT"), b"x").unwrap();
    fs::create_dir(directory.join("SomeDir")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("ZED.TXT", directory.join("LINK-NAME")).unwrap();

    let output = run_with_input(&directory, b"Y\n");
    assert!(output.status.success());
    #[cfg(target_os = "macos")]
    {
        assert!(directory.join("ZED.TXT").exists());
        assert!(directory.join("SomeDir").is_dir());
        assert!(String::from_utf8_lossy(&output.stderr).contains("skipped (exists): zed.txt"));
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(directory.join("zed.txt").exists());
        assert!(directory.join("somedir").is_dir());
        #[cfg(unix)]
        assert!(directory.join("link-name").is_symlink());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.find("link-name").unwrap() < text.find("somedir").unwrap());
    }
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(not(target_os = "macos"))]
#[test]
fn existing_destination_is_skipped_on_stderr() {
    let directory = temp_dir();
    fs::write(directory.join("README"), b"keep").unwrap();
    fs::write(directory.join("ReadMe"), b"source").unwrap();
    let output = run_with_input(&directory, b"y\n");
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("skipped (exists): readme"));
    assert!(directory.join("ReadMe").exists());
    fs::remove_dir_all(directory).unwrap();
}

fn run_with_input(directory: &std::path::Path, input: &[u8]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rnlower"));
    command
        .current_dir(directory)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
