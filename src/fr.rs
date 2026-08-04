//! Regex search/replace-across-a-file-tree behavior for the `fr` utility.

use crate::color::ColorMode;
use regex::bytes::Regex as BytesRegex;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const PROGRAM_NAME: &str = "fr";
const RED: &[u8] = b"\x1b[38;5;196m";
const RESET: &[u8] = b"\x1b[0m";
const YELLOW: &str = "38;5;220";

enum Mode {
    /// Single-argument search: highlight matches, write nothing.
    Search,
    /// `FROM TO`: highlight matches of `FROM`; `TO` is accepted but unused,
    /// matching the Go original where the show-only branch never reads it.
    Show,
    /// `FROM TO -f`/`--force`: replace matches of `FROM` with `TO` in place.
    Replace,
}

/// Runs `fr` and writes its process output to the supplied streams.
///
/// An invalid regex silently matches nothing across the whole run, mirroring
/// the Go original's per-call `regexp.Compile` failure handling.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    run_at(args, version, Path::new("."), stdout, stderr)
}

/// Runs `fr` rooted at `root` instead of the process's current directory;
/// the injectable seam tests use to avoid mutating global process state.
pub fn run_at<I, S, W, E>(args: I, version: &str, root: &Path, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if args.len() == 1 && is_flag(&args[0], "-v", "--version") {
        let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
        return 0;
    }

    let (pattern, replacement, mode) = match args.len() {
        1 => (args[0].to_string_lossy().into_owned(), None, Mode::Search),
        2 => (
            args[0].to_string_lossy().into_owned(),
            Some(args[1].to_string_lossy().into_owned()),
            Mode::Show,
        ),
        3 => {
            if !is_flag(&args[2], "-f", "--force") {
                let _ = writeln!(
                    stderr,
                    "Unrecognised flag {:?}. Only -f/--force is supported.",
                    args[2].to_string_lossy()
                );
                return 1;
            }
            (
                args[0].to_string_lossy().into_owned(),
                Some(args[1].to_string_lossy().into_owned()),
                Mode::Replace,
            )
        }
        _ => {
            let _ = write!(stderr, "{}", usage_text());
            return 1;
        }
    };

    let color = ColorMode::detect_stdout();
    let regex = BytesRegex::new(&pattern).ok();

    match walk(
        root,
        Path::new(""),
        regex.as_ref(),
        replacement.as_deref(),
        &mode,
        color,
        stdout,
    ) {
        Ok(()) => 0,
        Err(error) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: walk error: {error}");
            1
        }
    }
}

fn usage_text() -> String {
    format!(
        "Usage:\n\
  {PROGRAM_NAME} <REGEX>                -> search-only mode\n\
  {PROGRAM_NAME} <FROM> <TO>            -> show-only mode\n\
  {PROGRAM_NAME} <FROM> <TO> -f         -> replace-and-write mode\n"
    )
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}

/// Walks `root`/`relative` depth-first in sorted order, skipping hidden
/// directories (never descending into them) and non-regular entries
/// (symlinks included), matching `filepath.Walk`'s `Lstat`-based behavior.
/// A read error aborts the whole walk, matching the Go original's
/// `log.Fatalf` on any `walkFn` error.
fn walk<W: Write>(
    base: &Path,
    relative: &Path,
    regex: Option<&BytesRegex>,
    replacement: Option<&str>,
    mode: &Mode,
    color: ColorMode,
    stdout: &mut W,
) -> io::Result<()> {
    let dir_path = if relative.as_os_str().is_empty() {
        base.to_path_buf()
    } else {
        base.join(relative)
    };
    let mut entries: Vec<_> = fs::read_dir(&dir_path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let name = entry.file_name();
        let rel_path = relative.join(&name);
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            walk(base, &rel_path, regex, replacement, mode, color, stdout)?;
            continue;
        }
        if !file_type.is_file() {
            continue; // symlinks and other non-regular entries are skipped
        }

        let abs_path = base.join(&rel_path);
        if !is_text_file(&abs_path) {
            continue;
        }
        let Some(display_path) = rel_path.to_str() else {
            continue; // non-UTF-8 paths are outside repo convention; skip
        };

        process_file(
            &abs_path,
            display_path,
            regex,
            replacement,
            mode,
            color,
            stdout,
        )?;
    }
    Ok(())
}

fn process_file<W: Write>(
    path: &Path,
    display_path: &str,
    regex: Option<&BytesRegex>,
    replacement: Option<&str>,
    mode: &Mode,
    color: ColorMode,
    stdout: &mut W,
) -> io::Result<()> {
    let Some(regex) = regex else {
        return Ok(());
    };

    let original = fs::read(path)?;
    let occurrences = regex.find_iter(&original).count();
    if occurrences == 0 {
        return Ok(());
    }

    match mode {
        Mode::Replace => {
            let to = replacement.unwrap_or_default();
            let updated = regex.replace_all(&original, to.as_bytes()).into_owned();
            write_atomic(path, &updated)?;
            let _ = writeln!(
                stdout,
                "{}: {occurrences} occurrence(s) replaced",
                color.paint(YELLOW, display_path)
            );
        }
        Mode::Search | Mode::Show => {
            print_matching_lines(&original, display_path, regex, color, stdout)?;
        }
    }
    Ok(())
}

/// Splits `data` on `\n`, dropping one trailing `\r` per line, matching
/// `bufio.Scanner`'s default `ScanLines` split function.
fn split_lines(data: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut rest = data;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        if let Some(pos) = rest.iter().position(|&byte| byte == b'\n') {
            let mut line = &rest[..pos];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            rest = &rest[pos + 1..];
            Some(line)
        } else {
            let line = rest;
            rest = &rest[rest.len()..];
            Some(line)
        }
    })
}

fn print_matching_lines<W: Write>(
    data: &[u8],
    display_path: &str,
    regex: &BytesRegex,
    color: ColorMode,
    stdout: &mut W,
) -> io::Result<()> {
    for (index, line) in split_lines(data).enumerate() {
        if regex.is_match(line) {
            write!(
                stdout,
                "{}:{}: ",
                color.paint(YELLOW, display_path),
                index + 1
            )?;
            stdout.write_all(&highlight(line, regex, color))?;
            writeln!(stdout)?;
        }
    }
    Ok(())
}

fn highlight(line: &[u8], regex: &BytesRegex, color: ColorMode) -> Vec<u8> {
    if !color.enabled() {
        return line.to_vec();
    }
    let mut output = Vec::with_capacity(line.len());
    let mut last = 0;
    for found in regex.find_iter(line) {
        output.extend_from_slice(&line[last..found.start()]);
        output.extend_from_slice(RED);
        output.extend_from_slice(&line[found.start()..found.end()]);
        output.extend_from_slice(RESET);
        last = found.end();
    }
    output.extend_from_slice(&line[last..]);
    output
}

fn is_text_file(path: &Path) -> bool {
    let Ok(output) = Command::new("file")
        .args(["-b", "--mime-type"])
        .arg(path)
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let mime = String::from_utf8_lossy(&output.stdout);
    let mime = mime.trim();
    mime == "application/xml" || mime == "application/json" || mime.starts_with("text/")
}

/// Writes `data` to a `.tmp` sibling of `path` and renames it into place,
/// preserving `path`'s original permissions on the replacement.
fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let metadata = fs::metadata(path)?;
    let mut tmp_name = path.as_os_str().to_os_string();
    tmp_name.push(".tmp");
    let tmp_path = PathBuf::from(tmp_name);

    let mut tmp = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&tmp_path)?;
    tmp.write_all(data)?;
    tmp.flush()?;
    drop(tmp);

    set_mode(&tmp_path, &metadata)?;
    fs::rename(&tmp_path, path)
}

fn set_mode(path: &Path, metadata: &fs::Metadata) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(metadata.permissions().mode()),
        )?;
    }
    #[cfg(not(unix))]
    {
        fs::set_permissions(path, metadata.permissions())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rkit-fr-unit-{}-{number}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn split_lines_drops_trailing_cr_and_keeps_final_partial_line() {
        let lines: Vec<&[u8]> = split_lines(b"a\r\nb\nc").collect();
        assert_eq!(
            lines,
            vec![b"a".as_slice(), b"b".as_slice(), b"c".as_slice()]
        );
        let empty: Vec<&[u8]> = split_lines(b"").collect();
        assert!(empty.is_empty());
        let trailing_newline: Vec<&[u8]> = split_lines(b"a\nb\n").collect();
        assert_eq!(trailing_newline, vec![b"a".as_slice(), b"b".as_slice()]);
    }

    #[test]
    fn highlight_wraps_each_match_and_passes_through_when_color_disabled() {
        let regex = BytesRegex::new("foo").unwrap();
        let disabled = ColorMode::new(false);
        assert_eq!(highlight(b"foo bar foo", &regex, disabled), b"foo bar foo");

        let enabled = ColorMode::new(true);
        let highlighted = highlight(b"foo bar", &regex, enabled);
        assert_eq!(highlighted, b"\x1b[38;5;196mfoo\x1b[0m bar");
    }

    #[test]
    fn invalid_regex_matches_nothing_and_exits_zero() {
        let directory = temp_dir();
        fs::write(directory.join("note.txt"), b"anything").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_at(["("], "1.0.0", &directory, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn show_only_mode_ignores_the_to_argument() {
        let directory = temp_dir();
        fs::write(directory.join("note.txt"), b"foo\n").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_at(
            ["foo", "bar"],
            "1.0.0",
            &directory,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.contains("foo"));
        assert!(!text.contains("bar"));
        assert_eq!(fs::read(directory.join("note.txt")).unwrap(), b"foo\n");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn replace_mode_preserves_mode_and_skips_binary_files() {
        let directory = temp_dir();
        let text_path = directory.join("note.txt");
        let binary_path = directory.join("blob.bin");
        let original_binary = [0x00u8, 0xff, 0x00, 0x10];
        fs::write(&text_path, b"foo\nfoo\n").unwrap();
        fs::write(&binary_path, original_binary).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&text_path, fs::Permissions::from_mode(0o640)).unwrap();
        }

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_at(
            ["foo", "bar", "-f"],
            "1.0.0",
            &directory,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert_eq!(fs::read(&text_path).unwrap(), b"bar\nbar\n");
        assert_eq!(fs::read(&binary_path).unwrap(), original_binary);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&text_path).unwrap().permissions().mode() & 0o777,
                0o640
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn hidden_directories_are_skipped_but_hidden_files_are_not() {
        let directory = temp_dir();
        fs::create_dir(directory.join(".git")).unwrap();
        fs::write(directory.join(".git").join("config"), b"foo").unwrap();
        fs::write(directory.join(".env"), b"foo\n").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_at(["foo"], "1.0.0", &directory, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.contains(".env"));
        assert!(!text.contains(".git"));
        fs::remove_dir_all(directory).unwrap();
    }
}
