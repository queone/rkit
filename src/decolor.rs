//! ANSI SGR removal and stream routing for the `decolor` utility.

use crate::color::ColorMode;
use std::ffi::OsString;
use std::io::{self, IsTerminal, Read, Write};
use std::path::Path;

const PROGRAM_NAME: &str = "decolor";
const WHITE: &str = "38;5;15";

/// Runs `decolor` with the process stdin and supplied output streams.
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
    if args.len() == 1 {
        let argument = &args[0];
        if is_flag(argument, "-v", "--version") {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            return 0;
        }
        if is_flag(argument, "-?", "-h") || argument == "--help" {
            return write_stdout(stdout, help(ColorMode::detect_stdout(), version));
        }
        return read_file(Path::new(argument), stdout, stderr);
    }

    if !input_is_terminal {
        let mut bytes = Vec::new();
        if let Err(error) = input.read_to_end(&mut bytes) {
            let _ = writeln!(stderr, "Error reading from stdin: {error}");
        }
        return write_bytes(stdout, &clear_sgr(&bytes));
    }
    write_stdout(stdout, help(ColorMode::detect_stdout(), version))
}

fn read_file<W: Write, E: Write>(path: &Path, stdout: &mut W, stderr: &mut E) -> u8 {
    match std::fs::read(path) {
        Ok(bytes) => write_bytes(stdout, &clear_sgr(&bytes)),
        Err(error) => {
            let _ = writeln!(stderr, "Error reading file {}: {error}", path.display());
            1
        }
    }
}

fn write_stdout<W: Write>(stdout: &mut W, text: String) -> u8 {
    write_bytes(stdout, text.as_bytes())
}

fn write_bytes<W: Write>(stdout: &mut W, bytes: &[u8]) -> u8 {
    match stdout.write_all(bytes) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}

fn help(color: ColorMode, version: &str) -> String {
    let name = color.paint(WHITE, PROGRAM_NAME);
    let usage = color.paint(WHITE, "Usage");
    let options = color.paint(WHITE, "Options");
    let examples = color.paint(WHITE, "Examples");
    format!(
        "{name} v{version}\n\
Text decolorizer — https://github.com/queone/rkit\n\
{usage}\n\
  {name} [options] [file]\n\
\n\
  The file can be piped into the utility, or referenced as an argument.\n\
\n\
{options}\n\
  |piped input|       Piped text is decolorized\n\
  FILENAME            Decolorize the given file path\n\
  -v, --version       Print version and exit\n\
  -?, -h, --help      Show this help message and exit\n\
\n\
{examples}\n\
  cat file | {name}\n\
  {name} /path/to/file\n\
  {name} -h\n"
    )
}

/// Removes CSI SGR sequences while preserving all other input bytes.
pub fn clear_sgr(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index] == 0x1b && input.get(index + 1) == Some(&b'[') {
            let mut end = index + 2;
            while end < input.len() && (input[end].is_ascii_digit() || input[end] == b';') {
                end += 1;
            }
            if end < input.len() && input[end] == b'm' {
                index = end + 1;
                continue;
            }
        }
        output.push(input[index]);
        index += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn strips_basic_eight_bit_and_truecolor_sgr() {
        let input =
            b"\x1b[31mred\x1b[0m \x1b[38;5;242mgray\x1b[0m \x1b[38;2;30;144;255mblue\x1b[0m";
        assert_eq!(clear_sgr(input), b"red gray blue");
    }

    #[test]
    fn preserves_non_sgr_sequences_and_binary_bytes() {
        let input = b"a\x1b[2Kb\x00\xff\x1b[badmc";
        assert_eq!(clear_sgr(input), input);
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("simulated stream failure"))
        }
    }

    #[test]
    fn stdin_read_failure_is_diagnosed_but_non_fatal() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with_input(
            std::iter::empty::<&str>(),
            "1.1.1",
            &mut FailingReader,
            false,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert!(stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&stderr).contains("Error reading from stdin:"),
            "stderr: {}",
            String::from_utf8_lossy(&stderr)
        );
    }

    #[test]
    fn piped_input_ignores_extra_arguments() {
        let mut input = Cursor::new(b"\x1b[32mok\x1b[0m".to_vec());
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with_input(
            ["ignored", "arguments"],
            "1.1.1",
            &mut input,
            false,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert_eq!(stdout, b"ok");
        assert!(stderr.is_empty());
    }
}
