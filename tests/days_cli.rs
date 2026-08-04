use std::process::Command;

fn local_today() -> String {
    let output = Command::new("date").arg("+%Y-%m-%d").output().unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn today_in_zone(zone: &str) -> String {
    let output = Command::new("date")
        .env("TZ", zone)
        .arg("+%Y-%m-%d")
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn utc_today() -> String {
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_days"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"days v1.1.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn calculates_offsets_and_date_pairs() {
    let offset = Command::new(env!("CARGO_BIN_EXE_days"))
        .arg("0")
        .output()
        .unwrap();
    assert!(offset.status.success());
    assert_eq!(
        String::from_utf8(offset.stdout).unwrap().trim(),
        local_today()
    );

    let pair = Command::new(env!("CARGO_BIN_EXE_days"))
        .args(["2024-01-01", "2024-12-31"])
        .output()
        .unwrap();
    assert!(pair.status.success());
    assert_eq!(pair.stdout, b"365\n");

    let flexible = Command::new(env!("CARGO_BIN_EXE_days"))
        .args(["2024-fEb-28", "2024-03-01"])
        .output()
        .unwrap();
    assert!(flexible.status.success());
    assert_eq!(flexible.stdout, b"2\n");
}

#[test]
fn zero_offset_follows_a_pinned_time_zone() {
    for zone in ["Pacific/Kiritimati", "Etc/GMT+12"] {
        let before = today_in_zone(zone);
        let output = Command::new(env!("CARGO_BIN_EXE_days"))
            .env("TZ", zone)
            .arg("0")
            .output()
            .unwrap();
        let after = today_in_zone(zone);
        assert!(output.status.success());
        let printed = String::from_utf8(output.stdout).unwrap().trim().to_owned();
        assert!(
            printed == before || printed == after,
            "zone {zone}: printed {printed}, expected {before} or {after}"
        );
    }
}

#[test]
fn zero_offset_falls_back_to_utc_without_the_date_command() {
    let before = utc_today();
    let output = Command::new(env!("CARGO_BIN_EXE_days"))
        .env("PATH", "")
        .arg("0")
        .output()
        .unwrap();
    let after = utc_today();
    assert!(output.status.success());
    let printed = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    assert!(
        printed == before || printed == after,
        "printed {printed}, expected {before} or {after}"
    );
}

#[test]
fn invalid_operands_return_contextual_errors() {
    let output = Command::new(env!("CARGO_BIN_EXE_days"))
        .arg("not-a-date")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("days: bad date"));
    assert!(output.stdout.is_empty());
}

#[test]
fn documentation_describes_date_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## days"));
    assert!(readme.contains("days v1.1.0"));
    assert!(readme.contains("`YYYY-MMM-DD`"));
    assert!(readme.contains("Date comparisons use UTC"));
    assert!(readme.contains("read from the host `date` command with a UTC fallback"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### days"));
    assert!(arch.contains("(host `date` with a UTC fallback)"));
}

#[test]
fn help_is_available_without_operands() {
    let output = Command::new(env!("CARGO_BIN_EXE_days"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Calendar days calculator"));
}
