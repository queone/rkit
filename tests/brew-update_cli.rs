#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    path: PathBuf,
    log: PathBuf,
}

impl Fixture {
    fn new(casks: &str) -> Self {
        let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rkit-brew-update-{}-{number}", std::process::id()));
        fs::create_dir(&path).expect("create fixture directory");
        let log = path.join("calls.log");
        let cask_file = path.join("casks");
        let brew = path.join("brew");
        fs::write(&cask_file, casks).expect("write cask fixture");
        fs::write(
            &brew,
            "#!/bin/sh
{
  printf 'argc=%s' \"$#\"
  for arg in \"$@\"; do printf '<%s>' \"$arg\"; done
  printf '\\n'
} >> \"$BREW_LOG\"
if [ \"$BREW_FAIL\" = \"$1:$2\" ] || [ \"$BREW_FAIL\" = \"$1\" ]; then
  printf 'failed:%s\n' \"$*\" >&2
  exit 17
fi
case \"$1:$2\" in
  list:--cask) cat \"$BREW_CASKS\" ;;
  *) printf 'stdout:%s\n' \"$*\"; printf 'stderr:%s\n' \"$*\" >&2 ;;
esac
",
        )
        .expect("write fake brew");
        fs::set_permissions(&brew, fs::Permissions::from_mode(0o755))
            .expect("make fake brew executable");
        Self { path, log }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_brew-update"));
        command
            .env("PATH", format!("{}:/usr/bin:/bin", self.path.display()))
            .env("BREW_LOG", &self.log)
            .env("BREW_CASKS", self.path.join("casks"))
            .env_remove("BREW_FAIL")
            .env("NO_COLOR", "1");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("run brew-update")
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).expect("stdout is UTF-8")
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).expect("stderr is UTF-8")
}

#[test]
fn version_forms_do_not_require_brew() {
    for flag in ["-v", "--version"] {
        let fixture = Fixture::new("");
        let output = fixture
            .command()
            .env("PATH", fixture.path.join("missing"))
            .args([flag])
            .output()
            .expect("run brew-update");
        assert!(output.status.success(), "{}", stderr(&output));
        assert_eq!(stdout(&output), "brew-update v1.3.5\n");
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn workflow_batches_trimmed_casks_and_streams_child_output() {
    let fixture = Fixture::new("\n  alpha\n\nbeta  \n");
    let output = fixture.run(&[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        fixture.calls(),
        [
            "argc=1<update>",
            "argc=1<upgrade>",
            "argc=2<list><--cask>",
            "argc=3<upgrade><alpha><beta>",
            "argc=2<cleanup><-s>"
        ]
    );
    let text = stdout(&output);
    assert!(text.starts_with("brew-update 1.3.5\n\n==> brew update\n"));
    assert!(text.contains("==> brew upgrade\n"));
    assert!(text.contains("==> brew upgrade alpha beta\n"));
    assert!(text.contains("==> brew cleanup -s\n"));
    assert!(text.ends_with("\n✓ All updates completed successfully\n"));
    assert!(!text.contains('\x1b'));
    assert!(text.contains("stdout:update"));
    assert!(stderr(&output).contains("stderr:update"));
    assert!(!text.contains("stdout:list --cask"));
}

#[test]
fn empty_cask_list_skips_cask_upgrade() {
    let fixture = Fixture::new("");
    let output = fixture.run(&[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        fixture.calls(),
        [
            "argc=1<update>",
            "argc=1<upgrade>",
            "argc=2<list><--cask>",
            "argc=2<cleanup><-s>"
        ]
    );
    assert!(!stdout(&output).contains("==> brew upgrade "));
}

#[test]
fn only_exact_version_forms_bypass_workflow() {
    for args in [
        vec!["--help"],
        vec!["unknown"],
        vec!["-v", "extra"],
        vec!["--version", "extra"],
    ] {
        let fixture = Fixture::new("");
        let output = fixture.run(&args);
        assert!(
            output.status.success(),
            "args {args:?}: {}",
            stderr(&output)
        );
        assert_eq!(
            fixture.calls(),
            [
                "argc=1<update>",
                "argc=1<upgrade>",
                "argc=2<list><--cask>",
                "argc=2<cleanup><-s>"
            ]
        );
    }
}

#[test]
fn each_workflow_failure_stops_following_commands_and_has_recovery_guidance() {
    for (failure, context) in [
        ("update", "Error during brew update:"),
        ("upgrade", "Error during brew upgrade:"),
        ("list:--cask", "Error during cask upgrade:"),
        ("upgrade:alpha", "Error during cask upgrade:"),
        ("cleanup:-s", "Error during brew cleanup -s:"),
    ] {
        let fixture = Fixture::new("alpha\n");
        let output = fixture
            .command()
            .env("BREW_FAIL", failure)
            .output()
            .expect("run brew-update");
        assert_eq!(output.status.code(), Some(1), "failure {failure}");
        let diagnostic = stderr(&output);
        assert!(diagnostic.contains(context), "{diagnostic}");
        assert!(diagnostic.contains("failed") || diagnostic.contains("status 17"));
        assert!(
            diagnostic.contains("Homebrew")
                && diagnostic.contains("PATH")
                && diagnostic.contains("retry")
        );
        let calls = fixture.calls();
        let failed_index = calls
            .iter()
            .position(|call| match failure {
                "update" => call == "argc=1<update>",
                "upgrade" => call == "argc=1<upgrade>",
                "list:--cask" => call == "argc=2<list><--cask>",
                "upgrade:alpha" => call == "argc=2<upgrade><alpha>",
                "cleanup:-s" => call == "argc=2<cleanup><-s>",
                _ => false,
            })
            .unwrap();
        assert_eq!(calls.len(), failed_index + 1, "calls: {calls:?}");
    }
}

#[test]
fn missing_brew_reports_path_recovery() {
    let fixture = Fixture::new("");
    let output = fixture
        .command()
        .env(
            "PATH",
            format!("{}/missing:/usr/bin:/bin", fixture.path.display()),
        )
        .output()
        .expect("run brew-update");
    assert_eq!(output.status.code(), Some(1));
    let diagnostic = stderr(&output);
    assert!(diagnostic.contains("Error during brew update:"));
    assert!(
        diagnostic.contains("Homebrew")
            && diagnostic.contains("PATH")
            && diagnostic.contains("retry")
    );
}
