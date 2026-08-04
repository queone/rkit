//! Memorable password generation for the `pgen` utility.

use crate::color::ColorMode;
use openssl::rand::rand_bytes;
use std::collections::HashSet;
use std::ffi::OsString;
use std::io::{self, Write};

const PROGRAM_NAME: &str = "pgen";
const WHITE: &str = "38;5;15";
const DELIMITER: &str = "_";
const ALPHANUMERIC_LENGTH: usize = 16;
const CONSONANTS: &[u8] = b"BCDFGHJKLMNPQRSTVWXYZ";
const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const WORDLIST_TEXT: &str = include_str!("pgen_wordlist.txt");

/// Runs `pgen` and writes its process output to the supplied streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let mut word_count = 3usize;

    if args.len() != 1 && args.iter().any(|arg| is_flag(arg, "-v", "--version")) {
        let _ = writeln!(
            stderr,
            "{PROGRAM_NAME}: version flag cannot be combined with other operands"
        );
        return 2;
    }

    if args.len() == 1 {
        if is_flag(&args[0], "-v", "--version") {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            return 0;
        }
        if is_help_flag(&args[0]) {
            let _ = write!(stdout, "{}", help(ColorMode::detect_stdout(), version));
            return 0;
        }
        let value = args[0].to_string_lossy();
        match value.parse::<i64>() {
            Ok(number) if (1..=9).contains(&number) => word_count = number as usize,
            _ => {
                let _ = writeln!(stdout, "NUMBER must be 1 thru 9.");
                return 1;
            }
        }
    }

    match generate_passwords(word_count) {
        Ok(lines) => {
            for line in lines {
                let _ = writeln!(stdout, "{line}");
            }
            0
        }
        Err(error) => {
            let _ = writeln!(stderr, "pgen: generate password: {error}");
            1
        }
    }
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}

fn is_help_flag(value: &OsString) -> bool {
    value == "-?" || value == "-h" || value == "--help"
}

fn help(color: ColorMode, version: &str) -> String {
    let name = color.paint(WHITE, PROGRAM_NAME);
    let usage = color.paint(WHITE, "Usage");
    let options = color.paint(WHITE, "Options");
    format!(
        "{name} v{version}\n\
Memorable password generator — https://github.com/queone/rkit\n\
{usage}\n\
  {name} [option]\n\
\n\
{options}\n\
                     Without arguments it generates a 3-word memorable password phrase\n\
  NUMBER             Generates a NUMBER-word memorable password phrase\n\
                     For example, if NUMBER is '6' it generates a 6-word phrase\n\
                     Minimum is 1, maximum is 9\n\
  -v, --version      Print version and exit\n\
  -?, -h, --help     Print this usage page\n"
    )
}

fn generate_passwords(word_count: usize) -> io::Result<[String; 3]> {
    let words = pick_words(word_count)?;
    let diceware_line = words.join(DELIMITER);
    let strong_memorable = strong_memorable_password(&words)?;
    let alphanumeric = random_alphanumeric(ALPHANUMERIC_LENGTH)?;
    Ok([diceware_line, strong_memorable, alphanumeric])
}

fn wordlist() -> Vec<&'static str> {
    WORDLIST_TEXT
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

/// Selects `count` unique random words from the embedded EFF large wordlist.
fn pick_words(count: usize) -> io::Result<Vec<&'static str>> {
    let words = wordlist();
    let mut chosen = Vec::with_capacity(count);
    let mut seen = HashSet::with_capacity(count);
    while chosen.len() < count {
        let index = random_below(words.len() as u32)? as usize;
        let word = words[index];
        if seen.insert(word) {
            chosen.push(word);
        }
    }
    Ok(chosen)
}

/// Title-cases the first word, dash-joins the rest, and appends one
/// independently drawn digit to a word chosen independently of the digit
/// value. This deliberately diverges from the Go original, whose appended
/// digit always equaled the chosen word's index.
fn strong_memorable_password(words: &[&str]) -> io::Result<String> {
    if words.is_empty() {
        return Ok(String::new());
    }
    let mut parts: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
    parts[0] = title_case(&parts[0]);
    let position = random_below(parts.len() as u32)? as usize;
    let digit = random_below(10)?;
    parts[position].push_str(&digit.to_string());
    Ok(parts.join("-"))
}

fn title_case(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Generates a random alphanumeric password whose first character is a
/// capital consonant, selecting every character via rejection sampling. This
/// deliberately diverges from the Go original's biased `byte % 62` selection.
fn random_alphanumeric(length: usize) -> io::Result<String> {
    let mut result = String::with_capacity(length);
    if length == 0 {
        return Ok(result);
    }
    let consonant_index = random_below(CONSONANTS.len() as u32)? as usize;
    result.push(CONSONANTS[consonant_index] as char);
    for _ in 1..length {
        let index = random_below(ALPHANUMERIC.len() as u32)? as usize;
        result.push(ALPHANUMERIC[index] as char);
    }
    Ok(result)
}

/// Draws a uniformly distributed value in `0..bound` via rejection sampling,
/// avoiding the modulo bias of a plain `random % bound`.
fn random_below(bound: u32) -> io::Result<u32> {
    assert!(bound > 0, "bound must be positive");
    let limit = u32::MAX - (u32::MAX % bound);
    loop {
        let mut buffer = [0u8; 4];
        rand_bytes(&mut buffer).map_err(|error| io::Error::other(error.to_string()))?;
        let value = u32::from_le_bytes(buffer);
        if value < limit {
            return Ok(value % bound);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wordlist_has_7776_unique_entries() {
        let words = wordlist();
        assert_eq!(words.len(), 7776);
        let unique: HashSet<&str> = words.iter().copied().collect();
        assert_eq!(unique.len(), 7776);
    }

    #[test]
    fn title_case_capitalizes_only_the_first_letter() {
        assert_eq!(title_case("abacus"), "Abacus");
        assert_eq!(title_case("t-shirt"), "T-shirt");
    }

    #[test]
    fn random_below_stays_within_bound() {
        for _ in 0..500 {
            let value = random_below(7776).unwrap();
            assert!(value < 7776);
        }
    }

    #[test]
    fn pick_words_returns_unique_words_of_requested_count() {
        for count in [1, 3, 9] {
            let words = pick_words(count).unwrap();
            assert_eq!(words.len(), count);
            let unique: HashSet<&str> = words.iter().copied().collect();
            assert_eq!(unique.len(), count);
        }
    }
}
