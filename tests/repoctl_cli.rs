#![cfg(unix)]

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rkit-repoctl-{}-{number}", std::process::id()));
        fs::create_dir(&path).expect("create fixture directory");
        Self { path }
    }

    fn repo(&self, name: &str) {
        fs::create_dir_all(self.path.join(name).join(".git")).expect("create git fixture");
    }

    fn script(&self, name: &str, body: &str) {
        let path = self.path.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture script parent");
        }
        fs::write(&path, body).expect("write fixture script");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .expect("make fixture script executable");
    }

    fn command(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_repoctl"))
            .args(args)
            .current_dir(&self.path)
            .env("PATH", format!("{}:/usr/bin:/bin", self.path.display()))
            .env("NO_COLOR", "1")
            .output()
            .expect("run repoctl")
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

fn fake_git() -> &'static str {
    r##"#!/bin/sh
wait_for_release() {
  [ -n "$REPOCTL_STARTED" ] && : > "$REPOCTL_STARTED"
  while [ ! -e "$REPOCTL_RELEASE" ]; do sleep 0.01; done
}
if [ "$1" = "--version" ]; then
  echo "git fixture"
  exit 0
fi
repo=${PWD##*/}
case "$1:$2:$3" in
  remote:get-url:origin)
    case "$repo" in
      bits) echo "https://github.com/kquo/bits.git" ;;
      governa) echo "https://github.com/queone/governa.git" ;;
      no-origin) exit 1 ;;
    esac
    ;;
  branch:--show-current:)
    [ "$REPOCTL_WAIT" = "status" ] && wait_for_release
    [ "$REPOCTL_FAIL_STATUS" = "$repo" ] && { echo "branch failed" >&2; exit 6; }
    echo main
    ;;
  status:--porcelain:) [ "$repo" = "dirty" ] && echo " M file"; exit 0 ;;
  ls-remote::) [ "$REPOCTL_FAIL_REMOTE" = "$repo" ] && { echo "remote failed" >&2; exit 7; }; exit 0 ;;
  pull::)
    if [ "$REPOCTL_WAIT" = "pull" ]; then
      echo "pull-stdout"
      echo "pull-stderr" >&2
      wait_for_release
    fi
    [ "$REPOCTL_FAIL_PULL" = "$repo" ] && { echo "pull failed" >&2; exit 8; }
    echo "Already up to date."
    ;;
  clone:*)
    if [ "$REPOCTL_WAIT" = "clone" ]; then
      echo "clone-stdout"
      echo "clone-stderr" >&2
      wait_for_release
    fi
    [ "$REPOCTL_FAIL_CLONE" = "$3" ] && { echo "clone failed" >&2; exit 9; }
    mkdir -p "$3/.git"
    echo "cloned $2"
    ;;
esac
"##
}

fn wait_for_path(path: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn fake_gh() -> &'static str {
    r##"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "gh fixture"
  exit 0
fi
if [ "$1" = "repo" ] && [ "$2" = "list" ]; then
  echo governa
  echo bits
  exit 0
fi
echo "unexpected gh arguments: $*" >&2
exit 9
"##
}

#[test]
fn status_aliases_sort_complete_origin_and_omit_headers() {
    let fixture = Fixture::new();
    fixture.repo("governa");
    fixture.repo("bits");
    fixture.script("git", fake_git());

    let short = fixture.command(&["s"]);
    let long = fixture.command(&["status"]);
    assert!(short.status.success(), "{}", stderr(&short));
    assert!(long.status.success(), "{}", stderr(&long));
    assert_eq!(stdout(&short), stdout(&long));
    let text = stdout(&short);
    assert_eq!(text.lines().count(), 2);
    assert!(text.starts_with("==> bits"));
    assert!(text.contains("https://github.com/kquo/bits.git"));
    assert!(text.contains("https://github.com/queone/governa.git"));
    assert!(!text.contains("Repo"));
    assert!(!text.contains('\x1b'));
    let bits = text.lines().next().unwrap();
    let origin_start = bits.find("https://").unwrap();
    assert!(bits[..origin_start].ends_with("    "));
    assert!(bits.ends_with("👍 main"));
    assert!(!text.contains("Checking"));
}

#[test]
fn status_reports_clean_dirty_and_missing_origin() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.repo("dirty");
    fixture.repo("no-origin");
    fixture.script("git", fake_git());

    let output = fixture.command(&["status"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    let bits = text.lines().find(|line| line.contains("bits")).unwrap();
    let dirty = text.lines().find(|line| line.contains("dirty")).unwrap();
    let no_origin = text
        .lines()
        .find(|line| line.contains("no-origin"))
        .unwrap();
    assert!(bits.contains("https://github.com/kquo/bits.git"));
    assert!(dirty.contains("<no origin>"));
    assert!(no_origin.contains("<no origin>"));
    assert_eq!(text.lines().count(), 3);
    assert_eq!(text.matches("👍 main").count(), 2);
    assert_eq!(text.matches("❌ main").count(), 1);
    assert!(!text.lines().any(|line| line.starts_with("    ")));
}

#[test]
fn status_failure_uses_final_row_then_uncolored_detail() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.script("git", fake_git());

    let output = Command::new(env!("CARGO_BIN_EXE_repoctl"))
        .args(["s", "bits"])
        .current_dir(&fixture.path)
        .env("PATH", format!("{}:/usr/bin:/bin", fixture.path.display()))
        .env("NO_COLOR", "1")
        .env("REPOCTL_FAIL_STATUS", "bits")
        .output()
        .expect("run failed status");
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    let mut lines = text.lines();
    assert!(lines.next().unwrap().ends_with("Status failed"));
    assert!(lines.next().unwrap().starts_with("    git branch"));
    assert!(!text.contains("Checking"));
    assert!(!text.contains('\x1b'));
}

#[test]
fn pull_prints_summary_and_details_and_returns_failure_after_continuing() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.repo("governa");
    fixture.script("git", fake_git());

    let output = Command::new(env!("CARGO_BIN_EXE_repoctl"))
        .args(["p"])
        .current_dir(&fixture.path)
        .env("PATH", format!("{}:/usr/bin:/bin", fixture.path.display()))
        .env("NO_COLOR", "1")
        .env("REPOCTL_FAIL_PULL", "governa")
        .output()
        .expect("run repoctl pull");
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert_eq!(text.matches("Already up to date").count(), 1);
    assert!(!text.contains("Pulling"));
    assert!(text.contains("Pull failed"));
    assert!(text.contains("pull failed"));
    assert!(text.contains("bits") && text.contains("governa"));
}

#[test]
fn pull_subset_excludes_unselected_repositories() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.repo("governa");
    fixture.script("git", fake_git());

    let output = fixture.command(&["pull", "bits"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("bits"));
    assert!(!text.contains("governa"));
}

#[test]
fn pull_buffers_output_until_final_primary_row() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.script("git", fake_git());
    let release = fixture.path.join("release-pull");
    let started = fixture.path.join("started-pull");
    let captured = fixture.path.join("pull-output");
    let output_file = fs::File::create(&captured).expect("create output capture");

    let mut child = Command::new(env!("CARGO_BIN_EXE_repoctl"))
        .args(["p", "bits"])
        .current_dir(&fixture.path)
        .env("PATH", format!("{}:/usr/bin:/bin", fixture.path.display()))
        .env("NO_COLOR", "1")
        .env("REPOCTL_WAIT", "pull")
        .env("REPOCTL_RELEASE", &release)
        .env("REPOCTL_STARTED", &started)
        .stdout(output_file)
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn buffered pull");
    wait_for_path(&started);
    assert_eq!(fs::metadata(&captured).unwrap().len(), 0);
    fs::write(&release, "release").expect("release pull");
    assert!(child.wait().expect("wait for pull").success());

    let text = fs::read_to_string(captured).expect("read pull output");
    let mut lines = text.lines();
    assert!(lines.next().unwrap().ends_with("Pulled"));
    assert!(lines.any(|line| line == "    pull-stdout"));
    assert!(text.contains("    pull-stderr"));
    assert!(!text.contains("Pulling"));
}

#[test]
fn processing_rows_precede_work_and_child_details_stream_live() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.script("git", fake_git());
    fixture.script(
        "bits/build.sh",
        "#!/bin/sh\necho build-stdout\necho build-stderr >&2\nwhile [ ! -e \"$REPOCTL_RELEASE\" ]; do sleep 0.01; done\n",
    );

    for (command, action, prefix) in [
        (vec!["b", "bits"], "Building", "build"),
        (vec!["c", "kquo", "newrepo"], "Cloning", "clone"),
    ] {
        let release = fixture.path.join(format!("release-{action}"));
        let mut child = Command::new(env!("CARGO_BIN_EXE_repoctl"))
            .args(&command)
            .current_dir(&fixture.path)
            .env("PATH", format!("{}:/usr/bin:/bin", fixture.path.display()))
            .env("NO_COLOR", "1")
            .env("REPOCTL_WAIT", prefix)
            .env("REPOCTL_RELEASE", &release)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn repoctl");
        let stdout = child.stdout.take().expect("capture repoctl stdout");
        let mut reader = BufReader::new(stdout);
        let mut row = String::new();
        reader.read_line(&mut row).expect("read processing row");
        assert!(row.starts_with("==> ") && row.contains(action), "{row}");
        assert!(child.try_wait().expect("check repoctl state").is_none());

        let mut details = String::new();
        for _ in 0..2 {
            let mut line = String::new();
            reader.read_line(&mut line).expect("read streamed detail");
            details.push_str(&line);
        }
        assert!(
            details.contains(&format!("    {prefix}-stdout")),
            "{details}"
        );
        assert!(
            details.contains(&format!("    {prefix}-stderr")),
            "{details}"
        );
        assert!(child.try_wait().expect("check streamed child").is_none());

        fs::write(&release, "release").expect("release fixture command");
        let mut remainder = String::new();
        reader
            .read_to_string(&mut remainder)
            .expect("read remainder");
        let status = child.wait().expect("wait for repoctl");
        assert!(status.success(), "{remainder}");
    }
}

#[test]
fn build_preserves_force_tty_and_indents_child_ansi() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.script("git", fake_git());
    fixture.script(
        "bits/build.sh",
        "#!/bin/sh\nprintf 'force=%s\\n' \"${GOVERNA_FORCE_TTY-unset}\"\nif [ \"${GOVERNA_FORCE_TTY:-0}\" = 1 ]; then printf '\\033[31mcolored\\033[0m\\n'; else printf 'plain\\n'; fi\n",
    );

    for inherited in [Some("0"), Some("1"), Some(""), None] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_repoctl"));
        command
            .args(["b", "bits"])
            .current_dir(&fixture.path)
            .env("PATH", format!("{}:/usr/bin:/bin", fixture.path.display()))
            .env("NO_COLOR", "1");
        match inherited {
            Some(value) => {
                command.env("GOVERNA_FORCE_TTY", value);
            }
            None => {
                command.env_remove("GOVERNA_FORCE_TTY");
            }
        }
        let output = command.output().expect("run build environment fixture");
        assert!(output.status.success(), "{}", stderr(&output));
        let text = stdout(&output);
        assert!(text.contains(&format!("    force={}", inherited.unwrap_or("unset"))));
        if inherited == Some("1") {
            assert!(text.contains("    \x1b[31mcolored\x1b[0m"));
        } else {
            assert!(text.contains("    plain"));
            assert!(!text.contains('\x1b'));
        }
    }
}

#[test]
fn documentation_matches_repoctl_result_and_streaming_lifecycles() {
    let readme = include_str!("../README.md");
    let architecture = include_str!("../arch.md");

    for text in [readme, architecture] {
        assert!(text.contains("status") && text.contains("pull"));
        assert!(text.contains("build") && text.contains("clone"));
        assert!(text.contains("GOVERNA_FORCE_TTY"));
    }
    assert!(readme.contains("repoctl 0.3.0"));
    assert!(readme.contains("completed") && readme.contains("live processing row"));
    assert!(architecture.contains("Complete status and pull operations"));
}

#[test]
fn operation_failure_summaries_cover_remote_build_and_clone() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.script("git", fake_git());
    fixture.script("gh", fake_gh());

    let remote = Command::new(env!("CARGO_BIN_EXE_repoctl"))
        .args(["p", "bits"])
        .current_dir(&fixture.path)
        .env("PATH", format!("{}:/usr/bin:/bin", fixture.path.display()))
        .env("NO_COLOR", "1")
        .env("REPOCTL_FAIL_REMOTE", "bits")
        .output()
        .expect("run remote failure");
    assert_eq!(remote.status.code(), Some(1));
    assert!(stdout(&remote).contains("Remote unavailable"));

    fixture.script(
        "bits/build.sh",
        "#!/bin/sh\necho build failed >&2\nexit 7\n",
    );
    fs::set_permissions(
        fixture.path.join("bits/build.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("make failing build script executable");
    let build = fixture.command(&["b", "bits"]);
    assert_eq!(build.status.code(), Some(1));
    assert!(stdout(&build).contains("Build failed"));
    assert!(stdout(&build).contains("build failed"));

    let clone = Command::new(env!("CARGO_BIN_EXE_repoctl"))
        .args(["c", "queone", "newrepo"])
        .current_dir(&fixture.path)
        .env("PATH", format!("{}:/usr/bin:/bin", fixture.path.display()))
        .env("NO_COLOR", "1")
        .env("REPOCTL_FAIL_CLONE", "newrepo")
        .output()
        .expect("run clone failure");
    assert_eq!(clone.status.code(), Some(1));
    assert!(stdout(&clone).contains("Clone failed"));
    assert!(stdout(&clone).contains("clone failed"));
}

#[test]
fn build_runs_only_requested_subset_and_reports_missing_script() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.repo("governa");
    fixture.script("git", fake_git());
    fixture.script("bits/build.sh", "#!/bin/sh\necho build-details\n");
    fs::set_permissions(
        fixture.path.join("bits/build.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("make build script executable");

    let output = fixture.command(&["b", "bits"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("bits"));
    assert!(text.contains("Built"));
    assert!(text.contains("    build-details"));
    assert!(!text.contains("governa"));

    let output = fixture.command(&["build"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stdout(&output).contains("No build.sh"));
}

#[test]
fn clone_explicit_subset_sorts_origins_and_skips_existing_destination() {
    let fixture = Fixture::new();
    fixture.repo("existing");
    fixture.script("git", fake_git());

    let output = fixture.command(&["c", "kquo", "zeta", "existing"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("https://github.com/kquo/existing.git"));
    assert!(text.contains("https://github.com/kquo/zeta.git"));
    assert!(text.contains("Skipped"));
    assert!(text.contains("Cloned"));
    assert!(fixture.path.join("zeta/.git").is_dir());
}

#[test]
fn clone_without_subset_reads_repository_names_from_gh() {
    let fixture = Fixture::new();
    fixture.script("git", fake_git());
    fixture.script("gh", fake_gh());

    let output = fixture.command(&["clone", "queone"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.contains("https://github.com/queone/bits.git"));
    assert!(text.contains("https://github.com/queone/governa.git"));
    assert!(fixture.path.join("bits/.git").is_dir());
    assert!(fixture.path.join("governa/.git").is_dir());
}

#[test]
fn unknown_subset_is_rejected_before_operation() {
    let fixture = Fixture::new();
    fixture.repo("bits");
    fixture.script("git", fake_git());

    let output = fixture.command(&["s", "missing"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stdout(&output).is_empty());
    assert!(stderr(&output).contains("no matching immediate Git repository"));
}

#[test]
fn missing_provider_commands_and_invalid_clone_operands_have_recovery_guidance() {
    let fixture = Fixture::new();
    fixture.repo("bits");

    let missing_git = Command::new(env!("CARGO_BIN_EXE_repoctl"))
        .args(["s"])
        .current_dir(&fixture.path)
        .env("PATH", &fixture.path)
        .env("NO_COLOR", "1")
        .output()
        .expect("run repoctl without git");
    assert_eq!(missing_git.status.code(), Some(1));
    assert!(stderr(&missing_git).contains("install Git"));

    let missing_gh = Command::new(env!("CARGO_BIN_EXE_repoctl"))
        .args(["clone", "queone"])
        .current_dir(&fixture.path)
        .env("PATH", format!("{}:/usr/bin:/bin", fixture.path.display()))
        .env("NO_COLOR", "1")
        .output()
        .expect("run repoctl without gh");
    assert_eq!(missing_gh.status.code(), Some(1));
    assert!(stderr(&missing_gh).contains("GitHub CLI"));

    let invalid_clone = fixture.command(&["clone"]);
    assert_eq!(invalid_clone.status.code(), Some(2));
    assert!(stderr(&invalid_clone).contains("expected OWNER"));
}

#[test]
fn help_and_version_are_terminal_commands() {
    let fixture = Fixture::new();
    let version = fixture.command(&["--version"]);
    assert!(version.status.success());
    assert_eq!(stdout(&version), "repoctl 0.3.0\n");
    assert!(stderr(&version).is_empty());

    let help = fixture.command(&["--help"]);
    assert!(help.status.success());
    assert!(stdout(&help).contains("s, status"));
}
