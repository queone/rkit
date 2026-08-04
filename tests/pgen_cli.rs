use std::process::Command;

const CONSONANTS: &str = "BCDFGHJKLMNPQRSTVWXYZ";

fn lines_of(bytes: &[u8]) -> Vec<&str> {
    std::str::from_utf8(bytes).unwrap().lines().collect()
}

#[test]
fn version_is_terminal_and_exact() {
    let output = Command::new(env!("CARGO_BIN_EXE_pgen"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"pgen v1.2.3\n");
    assert!(output.stderr.is_empty());
}

#[test]
fn version_flag_combined_with_other_operands_is_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_pgen"))
        .args(["-v", "extra"])
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
fn help_is_available_and_exits_zero() {
    let output = Command::new(env!("CARGO_BIN_EXE_pgen"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Memorable password generator"));
}

#[test]
fn default_invocation_generates_three_unique_words() {
    let output = Command::new(env!("CARGO_BIN_EXE_pgen")).output().unwrap();
    assert!(output.status.success());
    let lines = lines_of(&output.stdout);
    assert_eq!(lines.len(), 3);
    let words: Vec<&str> = lines[0].split('_').collect();
    assert_eq!(words.len(), 3);
    assert_eq!(
        words.iter().collect::<std::collections::HashSet<_>>().len(),
        3
    );
    assert!(
        words
            .iter()
            .all(|word| word.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
    );
}

#[test]
fn number_operand_controls_word_count() {
    for count in 1..=9usize {
        let output = Command::new(env!("CARGO_BIN_EXE_pgen"))
            .arg(count.to_string())
            .output()
            .unwrap();
        assert!(output.status.success());
        let lines = lines_of(&output.stdout);
        let words: Vec<&str> = lines[0].split('_').collect();
        assert_eq!(words.len(), count, "word count for NUMBER={count}");
    }
}

#[test]
fn out_of_range_or_non_numeric_operand_reports_bounds_error() {
    for bad in ["0", "10", "abc", "-1"] {
        let output = Command::new(env!("CARGO_BIN_EXE_pgen"))
            .arg(bad)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "operand {bad}");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            "NUMBER must be 1 thru 9.\n"
        );
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn extra_operands_are_ignored_and_default_applies() {
    let output = Command::new(env!("CARGO_BIN_EXE_pgen"))
        .args(["5", "ignored"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let lines = lines_of(&output.stdout);
    let words: Vec<&str> = lines[0].split('_').collect();
    assert_eq!(words.len(), 3);
}

#[test]
fn strong_memorable_password_has_title_case_and_one_digit() {
    let output = Command::new(env!("CARGO_BIN_EXE_pgen")).output().unwrap();
    let lines = lines_of(&output.stdout);
    let line = lines[1];
    assert!(line.chars().next().unwrap().is_ascii_uppercase());
    let digit_count = line.chars().filter(char::is_ascii_digit).count();
    assert_eq!(digit_count, 1, "line: {line}");
}

#[test]
fn alphanumeric_password_is_sixteen_chars_with_consonant_first() {
    let output = Command::new(env!("CARGO_BIN_EXE_pgen")).output().unwrap();
    let lines = lines_of(&output.stdout);
    let password = lines[2];
    assert_eq!(password.chars().count(), 16);
    assert!(CONSONANTS.contains(password.chars().next().unwrap()));
    assert!(password.chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn documentation_describes_password_generator_behavior() {
    let readme = include_str!("../README.md");
    assert!(readme.contains("## pgen"));
    assert!(readme.contains("pgen v1.2.3"));
    assert!(readme.contains("NUMBER must be 1 thru 9."));
    let arch = include_str!("../arch.md");
    assert!(arch.contains("### pgen"));
    assert!(arch.contains("rejection sampling"));
}
