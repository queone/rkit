use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rkit-tree-{label}-{}-{number}", std::process::id()));
        fs::create_dir(&path).expect("create fixture directory");
        Self { path }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = make_readable(&self.path);
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn run(args: impl IntoIterator<Item = impl Into<OsString>>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_tree"));
    command.env("NO_COLOR", "1");
    command.args(args.into_iter().map(Into::into));
    command.output().expect("run tree binary")
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

#[test]
fn help_reports_usage_on_stdout() {
    let output = run(["--help", "--bad"]);
    assert!(output.status.success());
    assert!(stderr(&output).is_empty());
    let text = stdout(&output);
    assert!(text.starts_with(&format!("tree v{}\n", env!("CARGO_PKG_VERSION"))));
    assert!(text.contains("https://github.com/queone/rkit"));
    assert!(text.contains("-f, --full-path"));
    assert!(text.contains("--               End option parsing"));
}

#[test]
fn version_reports_cargo_version_on_stdout() {
    let output = run(["root", "--version", "--bad"]);
    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        format!("tree v{}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(stderr(&output).is_empty());
}

#[test]
fn unknown_option_before_terminal_flag_is_usage_error() {
    let output = run(["--bad", "--help"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains("unsupported option"));
    assert!(stderr(&output).contains("use --help"));
}

#[test]
fn renders_nested_tree_in_filename_order() {
    let fixture = Fixture::new("nested");
    fs::write(fixture.path.join("βeta.txt"), "").unwrap();
    fs::write(fixture.path.join(".hidden"), "").unwrap();
    fs::write(fixture.path.join("alpha.txt"), "").unwrap();
    fs::create_dir(fixture.path.join("nested")).unwrap();
    fs::write(fixture.path.join("nested").join("z.txt"), "").unwrap();

    let output = run([fixture.path.as_os_str()]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        stdout(&output),
        "├── .hidden\n├── alpha.txt\n├── nested\n│   └── z.txt\n└── βeta.txt\n"
    );
}

#[test]
fn full_path_option_works_after_directory() {
    let fixture = Fixture::new("full-path");
    fs::write(fixture.path.join("a.txt"), "").unwrap();

    let output = run([
        fixture.path.as_os_str(),
        OsString::from("--full-path").as_os_str(),
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let expected_path = fixture.path.join("a.txt");
    assert_eq!(
        stdout(&output),
        format!("└── a.txt    {}\n", expected_path.display())
    );
}

#[test]
fn option_terminator_allows_dash_prefixed_directory() {
    let parent = Fixture::new("dash-parent");
    let directory = parent.path.join("-directory");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("file"), "").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_tree"))
        .current_dir(&parent.path)
        .env("NO_COLOR", "1")
        .args(["--", "-directory"])
        .output()
        .expect("run tree binary");
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "└── file\n");
}

#[test]
fn nonexistent_root_is_runtime_error_without_stdout() {
    let fixture = Fixture::new("missing-parent");
    let missing = fixture.path.join("missing");
    let output = run([missing.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    let diagnostic = stderr(&output);
    assert!(diagnostic.contains("read directory"));
    assert!(diagnostic.contains("verify the path exists and is readable"));
}

#[cfg(unix)]
#[test]
fn non_utf8_name_is_runtime_error_without_stdout() {
    use std::os::unix::ffi::OsStringExt;

    let fixture = Fixture::new("non-utf8");
    let name = OsString::from_vec(vec![b'f', 0x80]);
    if let Err(error) = fs::write(fixture.path.join(name), "") {
        if error.kind() == std::io::ErrorKind::PermissionDenied || error.raw_os_error() == Some(92)
        {
            return;
        }
        panic!("create non-UTF-8 fixture: {error}");
    }

    let output = run([fixture.path.as_os_str()]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains("filename is not valid UTF-8"));
}

#[cfg(unix)]
#[test]
fn unreadable_descendant_warns_and_continues_when_enforced() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("unreadable");
    fs::write(fixture.path.join("before.txt"), "").unwrap();
    let nested = fixture.path.join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("inside.txt"), "").unwrap();
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o000)).unwrap();

    let output = run([fixture.path.as_os_str()]);
    make_readable(&nested).unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    if stdout(&output).contains("inside.txt") {
        return;
    }
    assert_eq!(stdout(&output), "├── before.txt\n└── nested\n");
    assert!(stderr(&output).contains("skip unreadable directory"));
    assert!(stderr(&output).contains("grant access"));
}

#[cfg(unix)]
fn make_readable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if path.exists() {
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                make_readable(&entry.path())?;
            }
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn make_readable(_path: &Path) -> std::io::Result<()> {
    Ok(())
}
