use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

fn temp_dir() -> PathBuf {
    let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
    let path =
        std::env::temp_dir().join(format!("rkit-retotal-cli-{}-{number}", std::process::id()));
    fs::create_dir(&path).unwrap();
    path
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_retotal"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"retotal v2.0.0\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn help_and_bad_argument_counts_show_usage_and_exit_zero() {
    for args in [vec!["-h"], vec!["--help"], vec![], vec!["a", "b"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_retotal"))
            .args(&args)
            .output()
            .unwrap();
        assert!(output.status.success(), "args {args:?}");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("retotal v2.0.0"));
        assert!(text.contains("Consolidate"));
    }
}

#[test]
fn consolidates_csv_input_into_stem_txt_with_totals_and_signature() {
    let directory = temp_dir();
    let input = directory.join("budget.csv");
    fs::write(
        &input,
        "TYPE,DESCRIPTION,MO/AVG,YR/AVG,NOTES\nIncome,Salary,5000,60000,primary\n,Rent,1200,14400,monthly\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_retotal"))
        .arg(&input)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("budget.txt"));

    let result = fs::read_to_string(directory.join("budget.txt")).unwrap();
    assert!(result.contains("Income - Salary"));
    assert!(result.contains("6,200.00"));
    assert!(result.contains("74,400.00"));
    assert!(
        result
            .trim_end()
            .ends_with("NOTE: To recalculate TOTALS for this FILE, run `retotal <FILE>`")
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn retallies_a_signed_output_file_in_place() {
    let directory = temp_dir();
    let budget = directory.join("budget.txt");
    fs::write(
        &budget,
        "DESCRIPTION  MO/AVG  YR/AVG  NOTES\nRent  1200  14400  monthly\nTOTAL  9999  99999\n\nNOTE: To recalculate TOTALS for this FILE, run `retotal <FILE>`\n",
    )
    .unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_retotal"))
        .arg(&budget)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());

    let result = fs::read_to_string(&budget).unwrap();
    let total_line = result
        .lines()
        .find(|line| line.trim_start().starts_with("TOTAL"))
        .unwrap();
    assert!(total_line.contains("1,200.00"));
    assert!(total_line.contains("14,400.00"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn retally_without_signature_fails_and_leaves_file_untouched() {
    let directory = temp_dir();
    let budget = directory.join("budget.txt");
    let body =
        "DESCRIPTION  MO/AVG  YR/AVG  NOTES\nRent  1200  14400  monthly\nTOTAL  1200  14400\n";
    fs::write(&budget, body).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_retotal"))
        .arg(&budget)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("NOTE: To recalculate TOTALS for this FILE, run `retotal <FILE>`")
    );
    assert_eq!(fs::read_to_string(&budget).unwrap(), body);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn consolidation_refuses_to_clobber_existing_output() {
    let directory = temp_dir();
    let input = directory.join("budget.csv");
    fs::write(
        &input,
        "TYPE,DESCRIPTION,MO/AVG,YR/AVG,NOTES\n,Rent,1200,14400,\n",
    )
    .unwrap();
    fs::write(directory.join("budget.txt"), "existing").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_retotal"))
        .arg(&input)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("already exists"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn missing_input_reports_diagnostic_and_exits_one() {
    let directory = temp_dir();
    let output = Command::new(env!("CARGO_BIN_EXE_retotal"))
        .arg(directory.join("missing.csv"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("retotal: read"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn documentation_describes_retotal_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## retotal"));
    assert!(readme.contains("retotal v2.0.0"));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### retotal"));
}
