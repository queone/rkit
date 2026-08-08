use std::process::Command;

// Only flag paths that never touch the real HOME/network are exercised
// here (`-v`, `-h`, an unknown flag): every other invocation resolves
// real filesystem paths via `store::Paths::from_env()` and would create
// `~/.local/state/cash5/` as a side effect on the machine running the
// test. Deeper behavior (daily run, stats, odds, fetch) is covered by
// `src/cash5/*.rs`'s inline unit tests against injected fakes instead.

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_cash5"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"cash5 v2.0.1\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_flag_prints_usage_not_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_cash5"))
        .arg("-h")
        .output()
        .unwrap();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("NJ Cash 5 daily numbers recommender"));
    assert!(text.contains("-m [N]"));
    assert!(text.contains("-o [N]"));
}

#[test]
fn unknown_flag_is_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_cash5"))
        .arg("--bogus")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown flag"));
}

#[test]
fn documentation_describes_cash5_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## cash5"));
    assert!(readme.contains("cash5 v2.0.1"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### cash5"));
}
