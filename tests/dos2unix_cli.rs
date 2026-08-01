use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "rkit-dos2unix-{label}-{}-{number}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create fixture directory");
        Self { path }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run(args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Output {
    Command::new(env!("CARGO_BIN_EXE_dos2unix"))
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run dos2unix binary")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

#[test]
fn previews_arbitrary_bytes_without_modifying_file() {
    let fixture = Fixture::new("preview");
    let file = fixture.path.join("mixed.txt");
    let input = b"a\r\nb\nc\rd\xff";
    fs::write(&file, input).unwrap();

    let output = run([file.as_os_str()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(output.stdout, b"a\\r\\n\nb\nc\rd\xff");
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read(file).unwrap(), input);
}

#[test]
fn force_forms_convert_byte_pairs_without_output() {
    for (label, args_before, args_after) in [
        ("short-before", vec![OsString::from("-f")], Vec::new()),
        ("long-after", Vec::new(), vec![OsString::from("--force")]),
    ] {
        let fixture = Fixture::new(label);
        let file = fixture.path.join("mixed.txt");
        fs::write(&file, b"a\r\nb\nc\rd\xff").unwrap();
        let mut args = args_before;
        args.push(file.as_os_str().to_owned());
        args.extend(args_after);

        let output = run(args);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(output.stdout.is_empty());
        assert!(output.stderr.is_empty());
        assert_eq!(fs::read(file).unwrap(), b"a\nb\nc\rd\xff");
    }
}

#[test]
fn option_terminator_allows_dash_prefixed_filename() {
    let fixture = Fixture::new("dash");
    let file = fixture.path.join("-input");
    fs::write(&file, b"a\r\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_dos2unix"))
        .current_dir(&fixture.path)
        .env("NO_COLOR", "1")
        .args(["--", "-input"])
        .output()
        .expect("run dos2unix binary");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(output.stdout, b"a\\r\\n\n");
}

#[test]
fn help_and_version_forms_are_successful_terminal_requests() {
    for flag in ["-h", "-?", "--help"] {
        let output = run([flag]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert!(output.stderr.is_empty());
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.starts_with("dos2unix v1.4.0\n"));
        assert!(text.contains("dos2unix [options] [--] FILE"));
        assert!(text.contains("-f, --force"));
    }
    for flag in ["-v", "--version"] {
        let output = run(["ignored", flag, "extra"]);
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(output.stdout, b"dos2unix v1.4.0\n");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn invalid_arguments_report_usage_exit_code() {
    for args in [
        Vec::<&str>::new(),
        vec!["--unknown"],
        vec!["first", "second"],
        vec!["-filename"],
    ] {
        let output = run(args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let diagnostic = stderr(&output);
        assert!(diagnostic.contains("use --help"));
    }
}

#[test]
fn missing_and_nonregular_operands_fail_without_output() {
    let fixture = Fixture::new("invalid-operands");
    let missing = fixture.path.join("missing");
    let output = run([missing.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("verify the operand"));

    let output = run([fixture.path.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let diagnostic = stderr(&output);
    assert!(diagnostic.contains("not a regular file"));
    assert!(diagnostic.contains("retry"));
}

#[cfg(unix)]
#[test]
fn conversion_preserves_symlink_hard_links_and_unix_mode() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};

    let fixture = Fixture::new("links");
    let target = fixture.path.join("target");
    let hard_link = fixture.path.join("hard-link");
    let symbolic_link = fixture.path.join("symbolic-link");
    fs::write(&target, b"a\r\n").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o640)).unwrap();
    fs::hard_link(&target, &hard_link).unwrap();
    symlink(&target, &symbolic_link).unwrap();
    let inode = fs::metadata(&target).unwrap().ino();

    let output = run([symbolic_link.as_os_str(), OsStr::new("--force")]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        fs::symlink_metadata(&symbolic_link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&target).unwrap(), b"a\n");
    assert_eq!(fs::read(&hard_link).unwrap(), b"a\n");
    let metadata = fs::metadata(&target).unwrap();
    assert_eq!(metadata.ino(), inode);
    assert_eq!(metadata.permissions().mode() & 0o777, 0o640);
}

#[cfg(unix)]
#[test]
fn dangling_symlink_fails_without_output() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new("dangling");
    let link = fixture.path.join("link");
    symlink(fixture.path.join("missing"), &link).unwrap();
    let output = run([link.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("verify the operand"));
}

#[cfg(unix)]
#[test]
fn accepts_non_utf8_content() {
    let fixture = Fixture::new("non-utf8");
    let file = fixture.path.join("input");
    fs::write(&file, b"\xff\r\n").unwrap();

    let output = run([file.as_os_str()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(output.stdout, b"\xff\\r\\n\n");
}

#[test]
fn readme_documents_ordered_macos_setup_and_both_binaries() {
    let readme = include_str!("../README.md");
    let ordered = [
        "xcode-select --print-path",
        "xcode-select --install",
        "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh",
        "default toolchain profile",
        "source \"$HOME/.cargo/env\"",
        "rustup --version",
        "rustc --version",
        "cargo --version",
        "rustup component add rustfmt clippy",
        "./build.sh",
    ];
    let mut previous = 0;
    for expected in ordered {
        let offset = readme[previous..]
            .find(expected)
            .unwrap_or_else(|| panic!("README is missing {expected:?}"));
        previous += offset + expected.len();
    }
    assert!(readme.contains("## Tree"));
    assert!(readme.contains("## dos2unix"));
    assert!(readme.contains("partially\nwritten"));
}
