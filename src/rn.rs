//! Bulk filename replacement with dry-run and forced-rename modes.

use crate::color::ColorMode;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::Write;

#[cfg(unix)]
use std::os::unix::ffi::{OsStrExt, OsStringExt};

const PROGRAM_NAME: &str = "rn";
const YELLOW: &str = "38;5;226";
const RED: &str = "38;5;124";
const GREEN: &str = "38;5;46";
const WHITE: &str = "38;5;15";

/// Run rn with injectable output streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, _stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if args.len() == 1 {
        match args[0].to_str() {
            Some("-v" | "--version") => {
                let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
                return 0;
            }
            Some("-h" | "-?" | "--help") => {
                usage(stdout, version);
                return 0;
            }
            _ => {}
        }
    }
    if !(1..=3).contains(&args.len()) {
        usage(stdout, version);
        return 0;
    }

    let old = &args[0];
    let new = args.get(1).cloned().unwrap_or_default();
    let do_rename = args.get(2).is_some_and(|option| option == "-f");
    let color = ColorMode::detect_stdout();
    if !do_rename {
        let _ = write!(
            stdout,
            "{}",
            color.paint(YELLOW, "DRY RUN: Re-run with '-f' option to execute.\n")
        );
    }

    let mut entries = match fs::read_dir(".") {
        Ok(entries) => entries.flatten().collect::<Vec<_>>(),
        Err(error) => {
            let message = format!("Error reading directory: {error}\n");
            let _ = write!(stdout, "{}", color.paint(RED, &message));
            return 1;
        }
    };
    entries.sort_by_key(|entry| entry.file_name());

    let mut found = false;
    for entry in entries {
        let is_directory = entry
            .file_type()
            .map(|file_type| file_type.is_dir())
            .unwrap_or(false);
        if is_directory {
            continue;
        }
        let old_name = entry.file_name();
        if !contains_name(&old_name, old) {
            continue;
        }
        found = true;
        let new_name = replace_name(&old_name, old, &new);
        let old_display = old_name.to_string_lossy();
        let new_display = new_name.to_string_lossy();
        if do_rename {
            match fs::rename(&old_name, &new_name) {
                Ok(()) => {
                    let message = format!("\"{old_display}\" -> \"{new_display}\"\n");
                    let _ = write!(stdout, "{}", color.paint(GREEN, &message));
                }
                Err(error) => {
                    let message =
                        format!("Failed to rename {old_display} -> {new_display}: {error}\n");
                    let _ = write!(stdout, "{}", color.paint(RED, &message));
                }
            }
        } else {
            let source = format!("\"{old_display}\"");
            let target = format!("\"{new_display}\"");
            let _ = writeln!(stdout, "{source:<60}  =>  {target}");
        }
    }

    if !found {
        let old_display = old.to_string_lossy();
        let message = format!("No filename has string '{old_display}'.\n");
        let _ = write!(stdout, "{}", color.paint(RED, &message));
        return 1;
    }
    0
}

fn usage(stdout: &mut impl Write, version: &str) {
    let color = ColorMode::detect_stdout();
    let name = color.paint(WHITE, PROGRAM_NAME);
    let usage = color.paint(WHITE, "Usage");
    let options = color.paint(WHITE, "Options");
    let examples = color.paint(WHITE, "Examples");
    let _ = write!(
        stdout,
        "{name} v{version}\nBulk file re-namer — https://github.com/queone/utils/blob/main/cmd/rn/README.md\n\n{usage}\n  {PROGRAM_NAME} \"OldString\" \"NewString\" [-f]\n\n  Renames all files in the current directory by replacing occurrences of OldString\n  in filenames with NewString. If NewString is empty (\"\"), the OldString is removed.\n\n{options}\n  -f                     Perform actual renaming (required to make changes).\n  -v, --version          Print version and exit.\n  -?, --help, -h         Show this help message and exit.\n\n{examples}\n  {PROGRAM_NAME} \"_draft\" \"\"           Show files that would be renamed (dry run).\n  {PROGRAM_NAME} \"_draft\" \"\" -f       Actually rename files.\n  {PROGRAM_NAME} \"temp\" \"final\" -f     Replace one substring with another.\n  {PROGRAM_NAME} -v                   Print version.\n  {PROGRAM_NAME} -h                   Display this help message.\n"
    );
}

fn contains_name(name: &OsStr, pattern: &OsStr) -> bool {
    #[cfg(unix)]
    {
        pattern.as_bytes().is_empty()
            || name
                .as_bytes()
                .windows(pattern.as_bytes().len())
                .any(|window| window == pattern.as_bytes())
    }
    #[cfg(not(unix))]
    name.to_string_lossy().contains(&pattern.to_string_lossy())
}

fn replace_name(name: &OsStr, pattern: &OsStr, replacement: &OsStr) -> OsString {
    #[cfg(unix)]
    {
        if let (Some(name), Some(pattern), Some(replacement)) =
            (name.to_str(), pattern.to_str(), replacement.to_str())
        {
            OsString::from(name.replace(pattern, replacement))
        } else {
            OsString::from_vec(replace_bytes(
                name.as_bytes(),
                pattern.as_bytes(),
                replacement.as_bytes(),
            ))
        }
    }
    #[cfg(not(unix))]
    OsString::from(
        name.to_string_lossy()
            .replace(&pattern.to_string_lossy(), &replacement.to_string_lossy()),
    )
}

#[cfg(unix)]
fn replace_bytes(source: &[u8], pattern: &[u8], replacement: &[u8]) -> Vec<u8> {
    if pattern.is_empty() {
        let mut result = Vec::with_capacity(source.len() + replacement.len() * (source.len() + 1));
        result.extend_from_slice(replacement);
        for byte in source {
            result.push(*byte);
            result.extend_from_slice(replacement);
        }
        return result;
    }
    let mut result = Vec::with_capacity(source.len());
    let mut index = 0;
    while index < source.len() {
        if source[index..].starts_with(pattern) {
            result.extend_from_slice(replacement);
            index += pattern.len();
        } else {
            result.push(source[index]);
            index += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::replace_name;
    use std::ffi::{OsStr, OsString};

    #[cfg(unix)]
    #[test]
    fn replacement_preserves_non_utf8_filename_bytes() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let name = OsString::from_vec(b"bad_\xff.txt".to_vec());
        let replacement = replace_name(&name, OsStr::new("_"), OsStr::new("-"));
        assert_eq!(replacement.as_bytes(), b"bad-\xff.txt");
    }
}
