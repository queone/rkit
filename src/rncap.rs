//! Interactive Unicode title-casing of every directory entry name.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{BufRead, Write};

const PROGRAM_NAME: &str = "rncap";

/// Run rncap with injectable streams.
pub fn run<I, S, R, W, E>(
    args: I,
    version: &str,
    input: &mut R,
    stdout: &mut W,
    stderr: &mut E,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    R: BufRead,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if args.len() == 1 && matches!(args[0].to_str(), Some("-v" | "--version")) {
        let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
        return 0;
    }

    let _ = write!(stdout, "Capitalize every file in CWD? Y/N ");
    if let Err(error) = stdout.flush() {
        return fail(stderr, "flush confirmation prompt", error.to_string());
    }
    let mut response = String::new();
    let _ = input.read_line(&mut response);
    if !matches!(response.trim(), "Y" | "y") {
        let _ = writeln!(stdout, "\nAborted.");
        return 1;
    }
    let _ = writeln!(stdout);

    let mut entries = match fs::read_dir(".") {
        Ok(entries) => entries.flatten().collect::<Vec<_>>(),
        Err(error) => return fail(stderr, "read directory", error.to_string()),
    };
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let old_name = entry.file_name();
        let new_name = OsString::from(title_case(&old_name));
        if new_name == old_name {
            continue;
        }
        if fs::metadata(&new_name).is_ok() {
            let _ = writeln!(stderr, "skipped (exists): {}", display(&new_name));
            continue;
        }
        if let Err(error) = fs::rename(&old_name, &new_name) {
            let _ = writeln!(
                stderr,
                "rename failed: {} -> {} ({error})",
                display(&old_name),
                display(&new_name)
            );
            continue;
        }
        let _ = writeln!(
            stdout,
            "'{}' -> '{}'",
            display(&old_name),
            display(&new_name)
        );
    }
    let _ = writeln!(stdout);
    0
}

fn title_case(name: &OsStr) -> String {
    let mut output = String::new();
    let mut capitalize_next = true;
    for character in name.to_string_lossy().chars() {
        if character.is_alphanumeric() {
            if capitalize_next {
                output.extend(character.to_uppercase());
                capitalize_next = false;
            } else {
                output.extend(character.to_lowercase());
            }
        } else {
            output.push(character);
            capitalize_next = true;
        }
    }
    output
}

fn display(name: &OsStr) -> String {
    name.to_string_lossy().into_owned()
}

fn fail(stderr: &mut impl Write, operation: &str, error: String) -> u8 {
    let _ = writeln!(stderr, "error: {operation}: {error}");
    1
}

#[cfg(test)]
mod tests {
    use super::{run, title_case};
    use std::ffi::OsStr;
    use std::io::{Cursor, Write};

    struct FlushWriter {
        bytes: Vec<u8>,
        flushes: usize,
    }

    impl Write for FlushWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    #[test]
    fn flushes_prompt_before_reading_confirmation() {
        let mut input = Cursor::new(b"N\n".to_vec());
        let mut stdout = FlushWriter {
            bytes: Vec::new(),
            flushes: 0,
        };
        let mut stderr = Vec::new();
        assert_eq!(
            run(
                Vec::<&str>::new(),
                "2.0.0",
                &mut input,
                &mut stdout,
                &mut stderr,
            ),
            1
        );
        assert_eq!(stdout.flushes, 1);
        assert!(
            stdout
                .bytes
                .starts_with(b"Capitalize every file in CWD? Y/N ")
        );
    }

    #[test]
    fn title_cases_unicode_words_after_punctuation() {
        assert_eq!(title_case(OsStr::new("élan CAFÉ.txt")), "Élan Café.Txt");
    }
}
