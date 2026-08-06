//! Financial TOTALS consolidation and re-tally behavior for the `retotal`
//! utility.

use crate::color::{ColorMode, WHITE10};
use regex::Regex;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

const PROGRAM_NAME: &str = "retotal";
/// Gates re-tally and tells the user how to recalculate. It is the last
/// non-empty line of every output file. `<FILE>` is a literal placeholder,
/// so the signature is path-independent — never substituted.
const SIGNATURE_LINE: &str = "NOTE: To recalculate TOTALS for this FILE, run `retotal <FILE>`";
const OUT_HEADER: [&str; 4] = ["DESCRIPTION", "MO/AVG", "YR/AVG", "NOTES"];

static TWO_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" {2,}").unwrap());
static QUOTED: LazyLock<Regex> = LazyLock::new(|| Regex::new("\"[^\"]*\"").unwrap());

#[derive(Default, Clone)]
struct Row {
    typ: String,
    desc: String,
    mo: String,
    yr: String,
    note: String,
}

type OutputRow = [String; 4];

/// Runs `retotal` and writes its process output to the supplied streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    run_with(args, version, ColorMode::detect_stdout(), stdout, stderr)
}

fn run_with<I, S, W, E>(
    args: I,
    version: &str,
    color: ColorMode,
    stdout: &mut W,
    stderr: &mut E,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();

    // Go has no -v/--version flag at all (a bare "--version" would be
    // treated as a literal, nonexistent file path); added here because
    // build.sh requires every utility's --version to print exactly
    // `name vX.Y.Z`, matching every other rkit utility's convention.
    if args.len() == 1 && (args[0] == "-v" || args[0] == "--version") {
        let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
        return 0;
    }

    if args.len() != 1 || args[0] == "-h" || args[0] == "--help" {
        let _ = write!(stdout, "{}", usage_text(version, color));
        return 0;
    }

    let path = Path::new(&args[0]);
    match dispatch(path) {
        Ok(Some(hint)) => {
            let _ = writeln!(stdout, "{hint}");
            0
        }
        Ok(None) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {message}");
            1
        }
    }
}

fn dispatch(path: &Path) -> Result<Option<String>, String> {
    if is_retotal_output(path)? {
        retally(path)?;
        Ok(None)
    } else {
        consolidate(path).map(Some)
    }
}

fn usage_text(version: &str, color: ColorMode) -> String {
    let name = color.paint(WHITE10, PROGRAM_NAME);
    let overview = color.paint(WHITE10, "Overview");
    format!(
        "{name} v{version}\n\
Financial TOTALS consolidator and re-tallier \u{2014} https://github.com/queone/rkit\n\
{overview}\n\
  retotal reads CSV or space-aligned financial data and writes an aligned summary with computed\n\
  TOTALS, signed with a recalculation note; it then re-tallies that signed output file in place\n\
  after you edit it. Supported invocations are:\n\
\n\
    {name} -h, --help    Prints this information screen.\n\
    {name} FILE          Consolidate CSV/aligned input into <stem>.txt with computed TOTALS and a\n\
                          signature line; or, when FILE is already a signed retotal output file,\n\
                          recompute its TOTALS in place.\n"
    )
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// Adds thousand separators only when `abs(value) >= 1000`; otherwise (or
/// on parse failure) returns the input unchanged. Preserves the input's own
/// decimal-place count rather than normalizing it — `normalize2` handles
/// that separately, upstream of every call site.
fn commatize(s: &str) -> String {
    let Ok(n) = s.parse::<f64>() else {
        return s.to_owned();
    };
    if n.abs() < 1000.0 {
        return s.to_owned();
    }
    if let Some(dot) = s.find('.') {
        let decimals = s.len() - dot - 1;
        return add_commas(&format!("{n:.decimals$}"));
    }
    add_commas(&format!("{}", n as i64))
}

fn add_commas(s: &str) -> String {
    let (negative, s) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let (int_part, dec_part) = match s.split_once('.') {
        Some((int_part, dec)) => (int_part, format!(".{dec}")),
        None => (s, String::new()),
    };
    let digits: Vec<char> = int_part.chars().collect();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.iter().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(*ch);
    }
    let mut result = grouped + &dec_part;
    if negative {
        result = format!("-{result}");
    }
    result
}

fn normalize2(s: &str) -> String {
    match s.parse::<f64>() {
        Ok(n) => format!("{n:.2}"),
        Err(_) => s.to_owned(),
    }
}

fn to_float(s: &str) -> f64 {
    s.replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// Parses one CSV record, handling `"quoted,fields"` with doubled-quote
/// escaping. Malformed quoting (a stray `"` outside a leading position) is
/// treated leniently rather than replicating Go's strict `LazyQuotes=false`
/// parse error — that path isn't exercised by any fixture.
fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                current.push(c);
            }
        } else if c == '"' && current.is_empty() {
            in_quotes = true;
        } else if c == ',' {
            fields.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    fields.push(current);
    fields
}

/// Reads `path` as CSV. Headers are case-normalized (uppercased) before
/// column lookup. Matches Go's `encoding/csv` field-count enforcement: a
/// data row whose field count doesn't match the header's silently
/// truncates reading (that row and everything after it is dropped), since
/// the Go original breaks its read loop unconditionally on the first
/// error. Blank lines are skipped, matching Go's default CSV behavior.
fn read_csv(path: &Path) -> Result<Vec<Row>, String> {
    let raw =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let content = strip_bom(&raw);
    let mut lines = content.split('\n').map(|line| line.trim_end_matches('\r'));

    let header_line = loop {
        match lines.next() {
            Some("") => continue,
            Some(line) => break line,
            None => {
                return Err(format!(
                    "read CSV headers from {}: empty file",
                    path.display()
                ));
            }
        }
    };
    let headers: Vec<String> = parse_csv_line(header_line)
        .iter()
        .map(|h| h.trim().to_uppercase())
        .collect();
    let expected = headers.len();

    let mut rows = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let fields = parse_csv_line(line);
        if fields.len() != expected {
            break;
        }
        let get = |name: &str| -> String {
            headers
                .iter()
                .position(|h| h == name)
                .and_then(|i| fields.get(i))
                .map(|v| v.trim().to_owned())
                .unwrap_or_default()
        };
        rows.push(Row {
            typ: get("TYPE"),
            desc: get("DESCRIPTION"),
            mo: get("MO/AVG"),
            yr: get("YR/AVG"),
            note: get("NOTES"),
        });
    }
    Ok(rows)
}

/// Splits `line` on 2+-space runs, padding short rows with empty trailing
/// columns and merging excess trailing parts back into the last column
/// with a `"  "` joiner (protects a field that itself contains a double
/// space).
fn split_aligned(line: &str, ncols: usize) -> Vec<String> {
    let mut parts: Vec<String> = TWO_SPACES.split(line.trim()).map(str::to_owned).collect();
    while parts.len() < ncols {
        parts.push(String::new());
    }
    if parts.len() > ncols && ncols > 0 {
        let tail = parts[ncols - 1..].join("  ");
        parts.truncate(ncols - 1);
        parts.push(tail);
    }
    parts
}

/// Reads `path` as space-aligned input. Headers are looked up
/// case-sensitively (no uppercasing) — a real asymmetry with `read_csv`,
/// not a bug. TYPE is inferred by splitting DESCRIPTION on the first
/// literal `" - "` substring, since aligned input has no TYPE column.
fn read_aligned(path: &Path) -> Result<Vec<Row>, String> {
    let raw =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let content = strip_bom(&raw);
    let lines: Vec<&str> = content
        .split('\n')
        .filter(|l| !l.trim().is_empty())
        .collect();
    let Some((header_line, data_lines)) = lines.split_first() else {
        return Ok(Vec::new());
    };

    let headers: Vec<&str> = TWO_SPACES.split(header_line.trim()).collect();
    let ncols = headers.len();

    let mut rows = Vec::new();
    for line in data_lines {
        let values = split_aligned(line, ncols);
        let get = |name: &str| -> String {
            headers
                .iter()
                .position(|h| *h == name)
                .and_then(|i| values.get(i))
                .cloned()
                .unwrap_or_default()
        };
        let mut desc = get("DESCRIPTION");
        let mut typ = String::new();
        if let Some(index) = desc.find(" - ") {
            typ = desc[..index].to_owned();
            desc = desc[index + 3..].to_owned();
        }
        rows.push(Row {
            typ,
            desc,
            mo: get("MO/AVG"),
            yr: get("YR/AVG"),
            note: get("NOTES"),
        });
    }
    Ok(rows)
}

/// Reads `path` as a prior `retotal` output file, dropping the trailing
/// signature line and any row whose DESCRIPTION contains "total"
/// (case-insensitive substring) — including a previous TOTAL row.
fn read_retotal_output(path: &Path) -> Result<Vec<Row>, String> {
    let raw =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let content = strip_bom(&raw);
    let lines: Vec<&str> = content
        .split('\n')
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return false;
            }
            line.trim_end_matches([' ', '\t', '\r']) != SIGNATURE_LINE
        })
        .collect();
    if lines.len() < 2 {
        return Ok(Vec::new());
    }

    let headers: Vec<&str> = TWO_SPACES.split(lines[0].trim()).collect();
    let ncols = headers.len();

    let mut rows = Vec::new();
    for line in &lines[1..] {
        let values = split_aligned(line, ncols);
        let get = |name: &str| -> String {
            headers
                .iter()
                .position(|h| *h == name)
                .and_then(|i| values.get(i))
                .cloned()
                .unwrap_or_default()
        };
        let desc = get("DESCRIPTION");
        if desc.to_lowercase().contains("total") {
            continue;
        }
        rows.push(Row {
            typ: String::new(),
            desc,
            mo: get("MO/AVG"),
            yr: get("YR/AVG"),
            note: get("NOTES"),
        });
    }
    Ok(rows)
}

/// Reports whether `path`'s first non-empty line is the retotal output
/// header (`DESCRIPTION`/`MO/AVG`/`YR/AVG`, 2+-space separated).
fn is_retotal_output(path: &Path) -> Result<bool, String> {
    let raw =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let content = strip_bom(&raw);
    for piece in content.splitn(2, '\n') {
        let line = piece.trim();
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&str> = TWO_SPACES.split(line).collect();
        return Ok(cols.len() >= 3
            && cols[0] == "DESCRIPTION"
            && cols[1] == "MO/AVG"
            && cols[2] == "YR/AVG");
    }
    Ok(false)
}

enum Format {
    Csv,
    Aligned,
}

/// Strips quoted substrings first, then checks for a remaining comma (→
/// CSV), else a 2+-space run (→ aligned), else defaults to CSV.
fn detect_input_format(first_line: &str) -> Format {
    let stripped = QUOTED.replace_all(first_line, "");
    if stripped.contains(',') {
        return Format::Csv;
    }
    if TWO_SPACES.is_match(first_line) {
        return Format::Aligned;
    }
    Format::Csv
}

/// Reports whether `content`'s last non-empty line is the signature line
/// (trailing whitespace tolerated).
fn has_signature(content: &str) -> bool {
    for line in content.split('\n').rev() {
        let trimmed_right = line.trim_end_matches([' ', '\t', '\r']);
        if trimmed_right.trim().is_empty() {
            continue;
        }
        return trimmed_right == SIGNATURE_LINE;
    }
    false
}

/// Derives the consolidation output filename by replacing `path`'s
/// extension with `.txt`.
fn stem_txt(path: &Path) -> PathBuf {
    path.with_extension("txt")
}

fn process(input: &[Row]) -> Vec<OutputRow> {
    let mut out = Vec::new();
    let mut mo_total = 0.0;
    let mut yr_total = 0.0;
    for r in input {
        if r.typ.to_lowercase().contains("total") || r.desc.to_lowercase().contains("total") {
            continue;
        }
        if r.typ.to_lowercase() == "type" && r.desc.to_lowercase() == "description" {
            continue;
        }
        let mo = r.mo.replace(',', "");
        let yr = r.yr.replace(',', "");
        mo_total += to_float(&mo);
        yr_total += to_float(&yr);
        let desc = if r.typ.is_empty() {
            r.desc.clone()
        } else {
            format!("{} - {}", r.typ, r.desc)
        };
        out.push([
            desc,
            commatize(&normalize2(&mo)),
            commatize(&normalize2(&yr)),
            r.note.clone(),
        ]);
    }
    out.push([
        "TOTAL".to_owned(),
        commatize(&format!("{mo_total:.2}")),
        commatize(&format!("{yr_total:.2}")),
        String::new(),
    ]);
    out
}

fn process_retally(input: &[Row]) -> Vec<OutputRow> {
    let mut out = Vec::new();
    let mut mo_total = 0.0;
    let mut yr_total = 0.0;
    for r in input {
        let mo = r.mo.replace(',', "");
        let yr = r.yr.replace(',', "");
        mo_total += to_float(&mo);
        yr_total += to_float(&yr);
        out.push([
            r.desc.clone(),
            commatize(&normalize2(&mo)),
            commatize(&normalize2(&yr)),
            r.note.clone(),
        ]);
    }
    out.push([
        "TOTAL".to_owned(),
        commatize(&format!("{mo_total:.2}")),
        commatize(&format!("{yr_total:.2}")),
        String::new(),
    ]);
    out
}

fn format_output(rows: &[OutputRow]) -> String {
    let mut widths = [0usize; 4];
    for (i, h) in OUT_HEADER.iter().enumerate() {
        widths[i] = widths[i].max(h.len());
    }
    for row in rows {
        for (i, v) in row.iter().enumerate() {
            widths[i] = widths[i].max(v.len());
        }
    }
    let right_align = [false, true, true, false];
    let pad = |s: &str, w: usize, right: bool| -> String {
        let fill = " ".repeat(w.saturating_sub(s.len()));
        if right {
            format!("{fill}{s}")
        } else {
            format!("{s}{fill}")
        }
    };
    let emit = |vals: &[String; 4]| -> String {
        let parts: Vec<String> = vals
            .iter()
            .enumerate()
            .map(|(i, v)| pad(v, widths[i], right_align[i]))
            .collect();
        parts.join("  ").trim_end().to_owned()
    };

    let mut out = String::new();
    out.push_str(&emit(&OUT_HEADER.map(String::from)));
    out.push('\n');
    for row in rows {
        out.push_str(&emit(row));
        out.push('\n');
    }
    out
}

fn with_signature(table: &str) -> String {
    format!("{table}\n{SIGNATURE_LINE}\n")
}

/// Validates the signature on an output-format file, recomputes TOTAL,
/// and rewrites the file in place. Leaves the file completely untouched
/// when the signature check fails.
fn retally(path: &Path) -> Result<(), String> {
    let raw =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    if !has_signature(strip_bom(&raw)) {
        return Err(format!(
            "{} is missing the required signature line; add this as the last line of the file:\n{SIGNATURE_LINE}",
            path.display()
        ));
    }

    let input = read_retotal_output(path)?;
    let result = with_signature(&format_output(&process_retally(&input)));
    write_rewrite(path, &result)
}

/// Reads CSV/aligned input, writes a signed `<stem>.txt` summary, and
/// returns a recalculation hint to print.
fn consolidate(path: &Path) -> Result<String, String> {
    let out_path = stem_txt(path);
    if out_path == path {
        return Err(format!(
            "input {} already uses the .txt output name; rename the input first",
            path.display()
        ));
    }
    if out_path.exists() {
        return Err(format!(
            "{} already exists; remove or rename it first",
            out_path.display()
        ));
    }

    let raw =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let content = strip_bom(&raw);
    let first_line = content
        .split('\n')
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");

    let input = match detect_input_format(first_line) {
        Format::Csv => read_csv(path)?,
        Format::Aligned => read_aligned(path)?,
    };

    let result = with_signature(&format_output(&process(&input)));
    write_new(&out_path, &result)?;
    Ok(format!(
        "wrote {}; run `retotal {}` to recalculate TOTALS",
        out_path.display(),
        out_path.display()
    ))
}

/// Creates a brand-new file at `0o644`.
#[cfg(unix)]
fn write_new(path: &Path, content: &str) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(path)
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn write_new(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|error| format!("write {}: {error}", path.display()))
}

/// Rewrites an existing file, preserving its current mode — matching
/// Go's `open(2)`-level semantics, where a requested mode only applies at
/// creation and is ignored for a file that already exists.
#[cfg(unix)]
fn write_rewrite(path: &Path, content: &str) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o644)
        .open(path)
        .map_err(|error| format!("write {}: {error}", path.display()))?;
    file.write_all(content.as_bytes())
        .map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn write_rewrite(path: &Path, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

    /// Must track `PROGRAM_VERSION` in `src/bin/retotal.rs`.
    const TEST_VERSION: &str = "2.0.0";

    fn temp_dir() -> PathBuf {
        let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rkit-retotal-unit-{}-{number}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }

    fn run_args(args: &[&str]) -> (u8, String, String) {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            args.to_vec(),
            TEST_VERSION,
            ColorMode::new(false),
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
    fn commatize_matches_go_cases() {
        let cases = [
            ("100", "100"),
            ("999", "999"),
            ("999.99", "999.99"),
            ("1000", "1,000"),
            ("1234", "1,234"),
            ("1234567", "1,234,567"),
            ("1234.56", "1,234.56"),
            ("12345.678", "12,345.678"),
            ("-1500", "-1,500"),
            ("-1500.25", "-1,500.25"),
            ("0", "0"),
            ("abc", "abc"),
            ("", ""),
        ];
        for (input, want) in cases {
            assert_eq!(commatize(input), want, "commatize({input:?})");
        }
    }

    #[test]
    fn to_float_strips_commas_and_defaults_to_zero() {
        assert_eq!(to_float("1234"), 1234.0);
        assert_eq!(to_float("1,234"), 1234.0);
        assert_eq!(to_float("1,234.56"), 1234.56);
        assert_eq!(to_float("abc"), 0.0);
        assert_eq!(to_float(""), 0.0);
    }

    #[test]
    fn is_retotal_output_distinguishes_formats() {
        let directory = temp_dir();
        let output = directory.join("output.txt");
        fs::write(
            &output,
            "DESCRIPTION  MO/AVG  YR/AVG  NOTES\nRent  1,200.00  14,400.00\n",
        )
        .unwrap();
        assert!(is_retotal_output(&output).unwrap());

        let csv = directory.join("input.csv");
        fs::write(
            &csv,
            "TYPE,DESCRIPTION,MO/AVG,YR/AVG,NOTES\nIncome,Salary,5000,60000,\n",
        )
        .unwrap();
        assert!(!is_retotal_output(&csv).unwrap());

        let aligned = directory.join("aligned.dat");
        fs::write(
            &aligned,
            "DESCRIPTION  MO/AVG  YR/AVG  NOTES\nIncome - Salary  5000  60000\n",
        )
        .unwrap();
        assert!(is_retotal_output(&aligned).unwrap());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn csv_input_merges_type_and_computes_totals() {
        let directory = temp_dir();
        let input = directory.join("in.csv");
        fs::write(
            &input,
            "TYPE,DESCRIPTION,MO/AVG,YR/AVG,NOTES\nIncome,Salary,5000,60000,primary\nIncome,Freelance,1500,18000,\n",
        )
        .unwrap();
        let (code, _, stderr) = run_args(&[input.to_str().unwrap()]);
        assert_eq!(code, 0, "stderr: {stderr}");
        let result = fs::read_to_string(directory.join("in.txt")).unwrap();
        assert!(result.contains("Income - Salary"));
        assert!(result.contains("Income - Freelance"));
        assert!(result.contains("6,500.00"));
        assert!(result.contains("78,000.00"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn aligned_input_extracts_type_and_computes_totals() {
        let directory = temp_dir();
        let input = directory.join("in.dat");
        fs::write(
            &input,
            "DESCRIPTION  TYPE  MO/AVG  YR/AVG  NOTES\nIncome - Salary  Inc  5000  60000  primary\nIncome - Freelance  Inc  1500  18000\n",
        )
        .unwrap();
        let (code, _, stderr) = run_args(&[input.to_str().unwrap()]);
        assert_eq!(code, 0, "stderr: {stderr}");
        let result = fs::read_to_string(directory.join("in.txt")).unwrap();
        assert!(result.contains("Income - Salary"));
        assert!(result.contains("6,500.00"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn skips_total_and_duplicate_header_rows() {
        let directory = temp_dir();
        let input = directory.join("in.csv");
        fs::write(
            &input,
            "TYPE,DESCRIPTION,MO/AVG,YR/AVG,NOTES\nTYPE,DESCRIPTION,MO/AVG,YR/AVG,NOTES\nIncome,Salary,5000,60000,\nTotal,All Income,5000,60000,\n,Grand total,5000,60000,\n",
        )
        .unwrap();
        let (code, _, stderr) = run_args(&[input.to_str().unwrap()]);
        assert_eq!(code, 0, "stderr: {stderr}");
        let result = fs::read_to_string(directory.join("in.txt")).unwrap();
        let data_rows = result
            .lines()
            .map(str::trim)
            .filter(|line| {
                !line.is_empty() && line != &SIGNATURE_LINE && !line.starts_with("DESCRIPTION")
            })
            .count();
        assert_eq!(data_rows, 2, "result:\n{result}");
        assert!(result.contains("5,000.00"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn precommatized_quoted_csv_values_parse_correctly() {
        let directory = temp_dir();
        let input = directory.join("in.csv");
        fs::write(
            &input,
            "TYPE,DESCRIPTION,MO/AVG,YR/AVG,NOTES\nIncome,Salary,\"5,000\",\"60,000\",\n",
        )
        .unwrap();
        let (code, _, stderr) = run_args(&[input.to_str().unwrap()]);
        assert_eq!(code, 0, "stderr: {stderr}");
        let result = fs::read_to_string(directory.join("in.txt")).unwrap();
        assert!(result.contains("5,000.00"));
        assert!(result.contains("60,000.00"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn csv_headers_are_case_insensitive() {
        let directory = temp_dir();
        let input = directory.join("in.csv");
        fs::write(
            &input,
            "Type,description,mo/avg,yr/avg,Notes\nHome,Property taxes,413.26,\"4,959.16\",Quarterly\n,Groceries,600,7200,\n",
        )
        .unwrap();
        let (code, _, stderr) = run_args(&[input.to_str().unwrap()]);
        assert_eq!(code, 0, "stderr: {stderr}");
        let result = fs::read_to_string(directory.join("in.txt")).unwrap();
        assert!(result.contains("Home - Property taxes"));
        assert!(result.contains("413.26"));
        assert!(result.contains("4,959.16"));
        assert!(result.contains("Quarterly"));
        assert!(result.contains("1,013.26"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn consolidation_writes_stem_output_with_signature_and_hint() {
        let directory = temp_dir();
        let input = directory.join("budget.csv");
        fs::write(
            &input,
            "TYPE,DESCRIPTION,MO/AVG,YR/AVG,NOTES\nIncome,Salary,5000,60000,primary\n,Rent,1200,14400,monthly\n",
        )
        .unwrap();
        let (code, stdout, _) = run_args(&[input.to_str().unwrap()]);
        assert_eq!(code, 0);
        assert!(stdout.contains("budget.txt"));
        let result = fs::read_to_string(directory.join("budget.txt")).unwrap();
        assert!(result.trim_end().ends_with(SIGNATURE_LINE));
        assert!(result.contains(&format!("\n\n{SIGNATURE_LINE}")));
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
        let (code, _, stderr) = run_args(&[input.to_str().unwrap()]);
        assert_eq!(code, 1);
        assert!(stderr.contains("already exists"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn consolidation_refuses_txt_extension_input() {
        let directory = temp_dir();
        let input = directory.join("budget.txt");
        fs::write(
            &input,
            "TYPE,DESCRIPTION,MO/AVG,YR/AVG,NOTES\n,Rent,1200,14400,\n",
        )
        .unwrap();
        let (code, _, stderr) = run_args(&[input.to_str().unwrap()]);
        assert_eq!(code, 1);
        assert!(stderr.contains("already uses the .txt output name"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retally_recomputes_totals_and_drops_old_total() {
        let directory = temp_dir();
        let budget = directory.join("budget.txt");
        fs::write(
            &budget,
            with_signature(
                "DESCRIPTION  MO/AVG  YR/AVG  NOTES\nRent  1200  14400  monthly\nTOTAL  9999  99999\n",
            ),
        )
        .unwrap();
        let (code, _, stderr) = run_args(&[budget.to_str().unwrap()]);
        assert_eq!(code, 0, "stderr: {stderr}");
        let result = fs::read_to_string(&budget).unwrap();
        let total_count = result
            .lines()
            .filter(|line| line.trim_start().starts_with("TOTAL"))
            .count();
        assert_eq!(total_count, 1, "result:\n{result}");
        let total_line = result
            .lines()
            .find(|line| line.trim_start().starts_with("TOTAL"))
            .unwrap();
        assert!(total_line.contains("1,200.00"));
        assert!(total_line.contains("14,400.00"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retally_normalizes_sloppy_entries() {
        let directory = temp_dir();
        let budget = directory.join("budget.txt");
        fs::write(
            &budget,
            with_signature(
                "DESCRIPTION  MO/AVG  YR/AVG  NOTES\nRent  1200  14400  monthly\nGroceries  600.5  7206\nInternet  89  1068\n",
            ),
        )
        .unwrap();
        let (code, _, stderr) = run_args(&[budget.to_str().unwrap()]);
        assert_eq!(code, 0, "stderr: {stderr}");
        let result = fs::read_to_string(&budget).unwrap();
        assert!(result.contains("600.50"));
        assert!(result.contains("89.00"));
        assert!(result.contains("1,889.50"));
        assert!(result.contains("22,674.00"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retally_requires_signature_and_leaves_file_untouched_on_failure() {
        let directory = temp_dir();
        let missing = directory.join("missing.txt");
        let body =
            "DESCRIPTION  MO/AVG  YR/AVG  NOTES\nRent  1200  14400  monthly\nTOTAL  1200  14400\n";
        fs::write(&missing, body).unwrap();
        let before = fs::read_to_string(&missing).unwrap();

        let (code, _, stderr) = run_args(&[missing.to_str().unwrap()]);
        assert_eq!(code, 1);
        assert!(stderr.contains(SIGNATURE_LINE));
        let after = fs::read_to_string(&missing).unwrap();
        assert_eq!(
            before, after,
            "file must not be modified on signature failure"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn retally_is_idempotent() {
        let directory = temp_dir();
        let budget = directory.join("b.txt");
        fs::write(
            &budget,
            with_signature("DESCRIPTION  MO/AVG  YR/AVG  NOTES\nRent  1200  14400  monthly\nTOTAL  1200  14400\n"),
        )
        .unwrap();
        run_args(&[budget.to_str().unwrap()]);
        let first = fs::read_to_string(&budget).unwrap();
        run_args(&[budget.to_str().unwrap()]);
        let second = fs::read_to_string(&budget).unwrap();
        assert_eq!(first, second);
        assert_eq!(second.matches(SIGNATURE_LINE).count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn signature_is_not_counted_as_a_data_row() {
        let directory = temp_dir();
        let budget = directory.join("b.txt");
        fs::write(
            &budget,
            with_signature(
                "DESCRIPTION  MO/AVG  YR/AVG  NOTES\nRent  1200  14400  monthly\nGroceries  600  7200\nTOTAL  0  0\n",
            ),
        )
        .unwrap();
        let (code, _, stderr) = run_args(&[budget.to_str().unwrap()]);
        assert_eq!(code, 0, "stderr: {stderr}");
        let result = fs::read_to_string(&budget).unwrap();
        assert_eq!(result.matches("NOTE: To recalculate").count(), 1);
        let total_line = result
            .lines()
            .find(|line| line.trim_start().starts_with("TOTAL"))
            .unwrap();
        assert!(total_line.contains("1,800.00"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn usage_screen_exits_zero_for_help_and_bad_argument_counts() {
        for args in [vec![], vec!["-h"], vec!["--help"], vec!["a", "b"]] {
            let (code, stdout, stderr) = run_args(&args);
            assert_eq!(code, 0, "args {args:?}, stderr: {stderr}");
            assert!(stdout.contains(&format!("retotal v{TEST_VERSION}")));
        }
    }

    #[test]
    fn output_alignment_has_no_trailing_spaces() {
        let rows = [[
            "Income - Salary".to_owned(),
            "5,000.00".to_owned(),
            "60,000.00".to_owned(),
            "primary".to_owned(),
        ]];
        let output = format_output(&rows);
        for line in output.lines() {
            assert_eq!(line, line.trim_end());
        }
    }
}
