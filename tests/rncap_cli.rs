use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> std::path::PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("rkit-rncap-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_rncap"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"rncap v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn prompt_abort_returns_one_without_mutation() {
    let directory = temp_dir();
    fs::write(directory.join("hello world.txt"), b"x").unwrap();
    let output = run_with_input(&directory, b"N\n", &[]);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stdout).contains("Capitalize every file in CWD? Y/N "));
    assert!(directory.join("hello world.txt").exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn acceptance_title_cases_sorted_entries_and_includes_directories() {
    let directory = temp_dir();
    fs::write(directory.join("hello WORLD.txt"), b"x").unwrap();
    fs::create_dir(directory.join("my folder")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("hello WORLD.txt", directory.join("link-name")).unwrap();

    let output = run_with_input(&directory, b"Y\n", &[]);
    assert!(output.status.success());
    #[cfg(target_os = "macos")]
    {
        assert!(directory.join("hello WORLD.txt").exists());
        assert!(directory.join("my folder").is_dir());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("skipped (exists): Hello World.Txt")
        );
    }
    #[cfg(not(target_os = "macos"))]
    {
        assert!(directory.join("Hello World.Txt").exists());
        assert!(directory.join("My Folder").is_dir());
        #[cfg(unix)]
        assert!(directory.join("Link-Name").is_symlink());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.find("Hello World.Txt").unwrap() < text.find("My Folder").unwrap());
    }
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(not(target_os = "macos"))]
#[test]
fn existing_destination_is_skipped_on_stderr() {
    let directory = temp_dir();
    fs::write(directory.join("Hello.Txt"), b"keep").unwrap();
    fs::write(directory.join("hello.txt"), b"source").unwrap();
    let output = run_with_input(&directory, b"y\n", &[]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("skipped (exists): Hello.Txt"));
    assert!(directory.join("hello.txt").exists());
    fs::remove_dir_all(directory).unwrap();
}

fn run_with_input(
    directory: &std::path::Path,
    input: &[u8],
    args: &[&str],
) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rncap"));
    command
        .args(args)
        .current_dir(directory)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
