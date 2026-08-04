//! JSON/YAML conversion and token-aware colorized-output behavior for the
//! `jy` utility.

use crate::color::{BLUE5, ColorMode, GREEN5, MAGENTA5, WHITE5, WHITE10, YELLOW5};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;
use yaml_rust2::scanner::{Scanner, Token, TokenType};
use yaml_rust2::yaml::{Hash, Yaml};
use yaml_rust2::{YamlEmitter, YamlLoader};

const PROGRAM_NAME: &str = "jy";

#[derive(Clone, Copy)]
enum Mode {
    /// Convert and colorize (default) or convert plainly (`-d`).
    Convert,
    Decolor,
}

/// Runs `jy` with the process stdin and supplied output streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let stdin = io::stdin();
    let mut input = stdin.lock();
    run_with_input(
        args,
        version,
        &mut input,
        stdin.is_terminal(),
        stdout,
        stderr,
    )
}

fn run_with_input<I, S, R, W, E>(
    args: I,
    version: &str,
    input: &mut R,
    input_is_terminal: bool,
    stdout: &mut W,
    stderr: &mut E,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    R: Read,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let color = ColorMode::detect_stdout();

    let mut decolorize = false;
    let mut colorize = false;
    let mut file_path: Option<OsString> = None;

    for arg in &args {
        if arg == "-d" {
            decolorize = true;
        } else if arg == "-c" {
            colorize = true;
        } else if is_flag(arg, "-v", "--version") {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            return 0;
        } else if arg == "-?" || arg == "-h" || arg == "--help" {
            let _ = write!(stdout, "{}", usage_text(version, color));
            return 0;
        } else {
            file_path = Some(arg.clone());
        }
    }

    if let Some(path) = file_path {
        let path = Path::new(&path);
        if colorize {
            return print_in_color(path, color, stdout, stderr);
        }
        let mode = if decolorize {
            Mode::Decolor
        } else {
            Mode::Convert
        };
        return process_file_input(path, mode, color, stdout, stderr);
    }

    if !input_is_terminal {
        let mut bytes = Vec::new();
        if let Err(error) = input.read_to_end(&mut bytes) {
            let _ = writeln!(stderr, "read stdin: {error}");
            return 1;
        }
        let mode = if decolorize {
            Mode::Decolor
        } else {
            Mode::Convert
        };
        return process_piped_input(&bytes, mode, color, stdout, stderr);
    }

    let _ = write!(stdout, "{}", usage_text(version, color));
    0
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}

fn usage_text(version: &str, color: ColorMode) -> String {
    let name = color.paint(WHITE10, PROGRAM_NAME);
    let usage = color.paint(WHITE10, "Usage");
    let options = color.paint(WHITE10, "Options");
    let examples = color.paint(WHITE10, "Examples");
    format!(
        "{name} v{version}\n\
JSON / YAML converter \u{2014} https://github.com/queone/rkit\n\
{usage}\n\
  {name} [options] [file]\n\
\n\
  Options can be specified in any order. The file can be piped into the utility, or it\n\
  can be referenced as an argument. If the file is YAML, the output will be JSON, or\n\
  vice versa.\n\
\n\
{options}\n\
  -c                     Colorize the output for the specified file.\n\
  -d                     Decolorize the output for piped input or file.\n\
  -v, --version          Print version and exit.\n\
  -?, --help, -h         Show this help message and exit.\n\
\n\
{examples}\n\
  cat file | {name}\n\
  {name} /path/to/file\n\
  {name} /path/to/file -d\n\
  {name} file.yaml -c        Prints a colorized version of the file. Does not convert.\n\
  {name} -h\n"
    )
}

fn file_usable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.len() > 0)
        .unwrap_or(false)
}

fn process_file_input<W: Write, E: Write>(
    path: &Path,
    mode: Mode,
    color: ColorMode,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    if !file_usable(path) {
        let _ = writeln!(stderr, "File is unusable");
        return 1;
    }
    let raw = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            let _ = writeln!(stderr, "Could not read file.");
            return 1;
        }
    };
    let cleared = crate::decolor::clear_sgr(&raw);
    print_out(&cleared, mode, color, stdout, stderr)
}

fn process_piped_input<W: Write, E: Write>(
    raw: &[u8],
    mode: Mode,
    color: ColorMode,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    let cleared = crate::decolor::clear_sgr(raw);
    print_out(&cleared, mode, color, stdout, stderr)
}

/// Detects JSON vs. YAML input and prints it in the other format, colorized
/// unless `mode` is [`Mode::Decolor`]. A bare `null`/empty document is
/// treated as neither format, matching the Go original (whose JSON- and
/// YAML-unmarshal-to-nil result is indistinguishable from a parse failure).
fn print_out<W: Write, E: Write>(
    raw: &[u8],
    mode: Mode,
    color: ColorMode,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    let Ok(text) = std::str::from_utf8(raw) else {
        let _ = writeln!(stderr, "Not JSON nor YAML");
        return 1;
    };

    if let Ok(value) = serde_json::from_str::<Value>(text)
        && !value.is_null()
    {
        let yaml_text = yaml_text_from_json(&value);
        return print_colorable(&yaml_text, mode, color, stdout);
    }

    let is_yaml = YamlLoader::load_from_str(text)
        .ok()
        .and_then(|docs| docs.into_iter().next())
        .filter(|doc| !matches!(doc, Yaml::Null | Yaml::BadValue))
        .map(|doc| yaml_to_json(&doc));

    let Some(value) = is_yaml else {
        let _ = writeln!(stderr, "Not JSON nor YAML");
        return 1;
    };

    let json_text = match serde_json::to_string_pretty(&value) {
        Ok(text) => text,
        Err(error) => {
            let _ = writeln!(stderr, "json reindent: {error}");
            return 1;
        }
    };
    print_colorable(&json_text, mode, color, stdout)
}

fn print_colorable<W: Write>(text: &str, mode: Mode, color: ColorMode, stdout: &mut W) -> u8 {
    match mode {
        Mode::Decolor => {
            let _ = writeln!(stdout, "{text}");
        }
        Mode::Convert => {
            let _ = writeln!(stdout, "{}", colorize_yaml(text, color));
        }
    }
    0
}

/// Loads `path` as YAML (which accepts JSON, a YAML subset) and prints its
/// raw, unconverted content colorized. Any read or parse failure — file
/// missing, unusable, or genuinely neither format — reports the same
/// message, matching the Go original's identical (redundant) retry path.
fn print_in_color<W: Write, E: Write>(
    path: &Path,
    color: ColorMode,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    let Ok(raw) = fs::read(path) else {
        let _ = writeln!(stderr, "File is neither JSON nor YAML");
        return 1;
    };
    let Ok(text) = std::str::from_utf8(&raw) else {
        let _ = writeln!(stderr, "File is neither JSON nor YAML");
        return 1;
    };
    if YamlLoader::load_from_str(text).is_err() {
        let _ = writeln!(stderr, "File is neither JSON nor YAML");
        return 1;
    }
    let _ = writeln!(stdout, "{}", colorize_yaml(text, color));
    0
}

// --------------------------------------------------------------------
// Yaml <-> serde_json::Value bridge (yaml-rust2 has no serde integration)
// --------------------------------------------------------------------

fn yaml_to_json(yaml: &Yaml) -> Value {
    match yaml {
        Yaml::Null | Yaml::BadValue => Value::Null,
        Yaml::Boolean(value) => Value::Bool(*value),
        Yaml::Integer(value) => Value::Number((*value).into()),
        Yaml::Real(text) => text
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(text.clone())),
        Yaml::String(text) => Value::String(text.clone()),
        Yaml::Array(items) => Value::Array(items.iter().map(yaml_to_json).collect()),
        Yaml::Hash(hash) => {
            let mut map = serde_json::Map::new();
            for (key, value) in hash {
                map.insert(yaml_key_to_string(key), yaml_to_json(value));
            }
            Value::Object(map)
        }
        Yaml::Alias(_) => Value::Null,
    }
}

fn yaml_key_to_string(key: &Yaml) -> String {
    match key {
        Yaml::String(text) => text.clone(),
        Yaml::Integer(value) => value.to_string(),
        Yaml::Real(text) => text.clone(),
        Yaml::Boolean(value) => value.to_string(),
        Yaml::Null => "null".to_owned(),
        _ => String::new(),
    }
}

fn json_to_yaml(value: &Value) -> Yaml {
    match value {
        Value::Null => Yaml::Null,
        Value::Bool(value) => Yaml::Boolean(*value),
        Value::Number(number) => {
            if let Some(value) = number.as_i64() {
                Yaml::Integer(value)
            } else {
                Yaml::Real(number.to_string())
            }
        }
        Value::String(text) => Yaml::String(text.clone()),
        Value::Array(items) => Yaml::Array(items.iter().map(json_to_yaml).collect()),
        Value::Object(map) => {
            let mut hash = Hash::new();
            for (key, value) in map {
                hash.insert(Yaml::String(key.clone()), json_to_yaml(value));
            }
            Yaml::Hash(hash)
        }
    }
}

fn yaml_text_from_json(value: &Value) -> String {
    let yaml = json_to_yaml(value);
    let mut text = String::new();
    let mut emitter = YamlEmitter::new(&mut text);
    let _ = emitter.dump(&yaml);
    match text.strip_prefix("---\n") {
        Some(rest) => rest.to_owned(),
        None => text.strip_prefix("---").unwrap_or(&text).to_owned(),
    }
}

// --------------------------------------------------------------------
// Token-aware colorizing
// --------------------------------------------------------------------

/// Colorizes YAML (or JSON, a valid YAML flow subset) source text by
/// walking `yaml-rust2`'s token stream and coloring the source span
/// between each pair of consecutive token markers: mapping keys and
/// mapping-value strings blue, other plain/quoted strings green (yellow
/// immediately after an anchor/alias), plain numbers/bools magenta,
/// anchors/aliases themselves yellow. `yaml-rust2` does not tokenize
/// comments, so comment text passes through uncolored.
fn colorize_yaml(source: &str, color: ColorMode) -> String {
    if !color.enabled() {
        return source.to_owned();
    }

    let scanner = Scanner::new(source.chars());
    let tokens: Vec<Token> = scanner.collect();
    if tokens.is_empty() {
        return source.to_owned();
    }

    // `yaml-rust2`'s scanner buffers structural tokens (e.g.
    // `BlockMappingStart`) via lookahead, so their marker can point past
    // tokens emitted after them. Sort by marker index (stable, so tied
    // markers keep emission order) to get well-ordered span boundaries;
    // `classify` still reasons about neighbors in emission order.
    let mut by_position: Vec<usize> = (0..tokens.len()).collect();
    by_position.sort_by_key(|&index| tokens[index].0.index());

    let end_of_source = source.len();
    let mut output = String::with_capacity(source.len() + 64);
    for position in 0..by_position.len() {
        let emission_index = by_position[position];
        let start = tokens[emission_index].0.index();
        let end = by_position
            .get(position + 1)
            .map(|&next| tokens[next].0.index())
            .unwrap_or(end_of_source);
        if start >= end
            || end > end_of_source
            || !source.is_char_boundary(start)
            || !source.is_char_boundary(end)
        {
            continue;
        }
        let span = &source[start..end];
        let code = classify(&tokens, emission_index);
        output.push_str(&color.paint(code, span));
    }
    output
}

/// Classifies the token at `index` (in scanner emission order). Every
/// token gets a color — Go's colorizer defaults to white for anything not
/// specifically classified (mapping keys/values, numbers/bools,
/// anchors/aliases), so punctuation and structural spans stay white too
/// rather than passing through unstyled.
fn classify(tokens: &[Token], index: usize) -> &'static str {
    match &tokens[index].1 {
        TokenType::Anchor(_) | TokenType::Alias(_) => YELLOW5,
        TokenType::Scalar(style, text) => {
            let is_key = matches!(
                tokens.get(index + 1).map(|token| &token.1),
                Some(TokenType::Value)
            );
            if is_key {
                return BLUE5;
            }
            let prev_is_anchor_or_alias = index > 0
                && matches!(
                    tokens[index - 1].1,
                    TokenType::Anchor(_) | TokenType::Alias(_)
                );
            if prev_is_anchor_or_alias {
                return YELLOW5;
            }
            if *style == yaml_rust2::scanner::TScalarStyle::Plain && is_number_or_bool(text) {
                return MAGENTA5;
            }
            GREEN5
        }
        _ => WHITE5,
    }
}

fn is_number_or_bool(text: &str) -> bool {
    matches!(
        YamlLoader::load_from_str(text)
            .ok()
            .and_then(|docs| docs.into_iter().next()),
        Some(Yaml::Integer(_)) | Some(Yaml::Real(_)) | Some(Yaml::Boolean(_))
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_bytes(args: &[&str], input: &[u8], is_terminal: bool) -> (u8, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut reader = input;
        let code = run_with_input(
            args.to_vec(),
            "1.0.0",
            &mut reader,
            is_terminal,
            &mut stdout,
            &mut stderr,
        );
        (
            code,
            String::from_utf8(stdout).unwrap(),
            String::from_utf8(stderr).unwrap(),
        )
    }

    #[test]
    fn json_to_yaml_and_back_round_trips_via_bridge() {
        let value = serde_json::json!({"b": 1, "a": [true, 2.5, "x", null]});
        let yaml = json_to_yaml(&value);
        let back = yaml_to_json(&yaml);
        assert_eq!(value, back);
    }

    #[test]
    fn bare_null_is_treated_as_neither_format() {
        let (code, stdout, stderr) = run_bytes(&[], b"null", false);
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert_eq!(stderr, "Not JSON nor YAML\n");
    }

    #[test]
    fn json_input_converts_to_yaml_and_yaml_converts_to_json() {
        let (code, stdout, _) = run_bytes(&["-d"], b"{\"a\": 1, \"b\": [2, 3]}", false);
        assert_eq!(code, 0);
        assert!(stdout.contains("a: 1"));
        assert!(stdout.contains("- 2"));

        let (code, stdout, _) = run_bytes(&["-d"], b"a: 1\nb:\n  - 2\n  - 3\n", false);
        assert_eq!(code, 0);
        assert!(stdout.contains("\"a\": 1"));
        assert!(stdout.contains("\"b\": ["));
    }

    #[test]
    fn colorize_marks_keys_blue_and_values_green() {
        let colored = colorize_yaml("key: value\n", ColorMode::new(true));
        assert!(colored.contains(&format!("\x1b[{BLUE5}mkey\x1b[0m")));
        assert!(colored.contains(&format!("\x1b[{GREEN5}mvalue")));
    }

    #[test]
    fn colorize_marks_plain_numbers_and_bools_magenta() {
        let colored = colorize_yaml("n: 42\nb: true\n", ColorMode::new(true));
        assert!(colored.contains(&format!("\x1b[{MAGENTA5}m42")));
        assert!(colored.contains(&format!("\x1b[{MAGENTA5}mtrue")));
    }

    #[test]
    fn colorize_marks_anchor_and_referenced_value_yellow() {
        let colored = colorize_yaml("a: &anchor value\nb: *anchor\n", ColorMode::new(true));
        assert!(colored.contains(&format!("\x1b[{YELLOW5}m&anchor")));
        assert!(colored.contains(&format!("\x1b[{YELLOW5}mvalue")));
        assert!(colored.contains(&format!("\x1b[{YELLOW5}m*anchor")));
    }

    #[test]
    fn colorize_disabled_returns_source_unchanged() {
        let source = "key: value\n";
        assert_eq!(colorize_yaml(source, ColorMode::new(false)), source);
    }

    #[test]
    fn file_input_missing_reports_unusable() {
        let directory = std::env::temp_dir().join(format!("rkit-jy-unit-{}", std::process::id()));
        let _ = fs::create_dir(&directory);
        let missing = directory.join("missing.json");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = process_file_input(
            &missing,
            Mode::Convert,
            ColorMode::new(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 1);
        assert_eq!(stderr, b"File is unusable\n");
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn no_file_and_terminal_stdin_prints_usage() {
        let (code, stdout, stderr) = run_bytes(&[], b"", true);
        assert_eq!(code, 0);
        assert!(stdout.contains("JSON / YAML converter"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn multiple_file_operands_use_the_last_one() {
        let directory =
            std::env::temp_dir().join(format!("rkit-jy-unit-last-{}", std::process::id()));
        let _ = fs::create_dir(&directory);
        fs::write(directory.join("first.json"), b"{\"first\": true}").unwrap();
        fs::write(directory.join("second.json"), b"{\"second\": true}").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with_input(
            vec![
                directory.join("first.json").to_string_lossy().into_owned(),
                directory.join("second.json").to_string_lossy().into_owned(),
                "-d".to_owned(),
            ],
            "1.0.0",
            &mut &b""[..],
            false,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.contains("second: true"));
        assert!(!text.contains("first"));
        let _ = fs::remove_dir_all(directory);
    }
}
