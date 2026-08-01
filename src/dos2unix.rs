//! CRLF preview and conversion behavior for the `dos2unix` binary.

use crate::color::ColorMode;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const PROGRAM_NAME: &str = "dos2unix";
pub const PROGRAM_VERSION: &str = "1.4.0";
const WHITE: &str = "38;5;15";

/// A command-line or filesystem failure with its process exit code.
#[derive(Debug)]
pub struct CliError {
    message: String,
    exit_code: u8,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 2,
        }
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
        }
    }

    /// Returns the diagnostic bytes intended for standard error.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the process exit code associated with this failure.
    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
}

/// Complete buffered output for a successful command.
#[derive(Debug, Eq, PartialEq)]
pub struct RunOutput {
    stdout: Vec<u8>,
}

impl RunOutput {
    fn empty() -> Self {
        Self { stdout: Vec::new() }
    }

    fn from_stdout(stdout: Vec<u8>) -> Self {
        Self { stdout }
    }

    /// Returns successful standard output without requiring UTF-8 file content.
    pub fn stdout(&self) -> &[u8] {
        &self.stdout
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Help,
    Version,
    Preview(PathBuf),
    Convert(PathBuf),
}

#[derive(Debug)]
enum FileFailure {
    BeforeWrite {
        operation: &'static str,
        error: io::Error,
    },
    AfterTruncate {
        operation: &'static str,
        error: io::Error,
    },
}

trait FileAccess {
    fn preview(&self, path: &Path) -> Result<Vec<u8>, FileFailure>;
    fn convert(&self, path: &Path) -> Result<(), FileFailure>;
}

struct Filesystem;

impl FileAccess for Filesystem {
    fn preview(&self, path: &Path) -> Result<Vec<u8>, FileFailure> {
        validate_path_regular(path).map_err(|error| FileFailure::BeforeWrite {
            operation: "inspect",
            error,
        })?;
        let mut file = File::open(path).map_err(|error| FileFailure::BeforeWrite {
            operation: "open",
            error,
        })?;
        validate_regular(&file).map_err(|error| FileFailure::BeforeWrite {
            operation: "inspect",
            error,
        })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| FileFailure::BeforeWrite {
                operation: "read",
                error,
            })?;
        Ok(bytes)
    }

    fn convert(&self, path: &Path) -> Result<(), FileFailure> {
        validate_path_regular(path).map_err(|error| FileFailure::BeforeWrite {
            operation: "inspect",
            error,
        })?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| FileFailure::BeforeWrite {
                operation: "open for conversion",
                error,
            })?;
        validate_regular(&file).map_err(|error| FileFailure::BeforeWrite {
            operation: "inspect",
            error,
        })?;

        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| FileFailure::BeforeWrite {
                operation: "read",
                error,
            })?;
        let converted = convert_bytes(&bytes);
        file.seek(SeekFrom::Start(0))
            .map_err(|error| FileFailure::BeforeWrite {
                operation: "prepare conversion",
                error,
            })?;
        file.set_len(0).map_err(|error| FileFailure::BeforeWrite {
            operation: "truncate",
            error,
        })?;
        file.write_all(&converted)
            .map_err(|error| FileFailure::AfterTruncate {
                operation: "write",
                error,
            })?;
        file.flush().map_err(|error| FileFailure::AfterTruncate {
            operation: "flush",
            error,
        })
    }
}

fn validate_path_regular(path: &Path) -> io::Result<()> {
    if path.metadata()?.is_file() {
        Ok(())
    } else {
        Err(not_regular_error())
    }
}

fn validate_regular(file: &File) -> io::Result<()> {
    if file.metadata()?.is_file() {
        Ok(())
    } else {
        Err(not_regular_error())
    }
}

fn not_regular_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "operand target is not a regular file",
    )
}

/// Runs `dos2unix` for the provided argument sequence.
pub fn run<I, S>(args: I) -> Result<RunOutput, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    run_with(args, &Filesystem, ColorMode::detect_stdout())
}

fn run_with<I, S, F>(args: I, files: &F, color: ColorMode) -> Result<RunOutput, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    F: FileAccess,
{
    match parse_args(args)? {
        Command::Help => Ok(RunOutput::from_stdout(help(color).into_bytes())),
        Command::Version => Ok(RunOutput::from_stdout(
            format!("{PROGRAM_NAME} v{PROGRAM_VERSION}\n").into_bytes(),
        )),
        Command::Preview(path) => files
            .preview(&path)
            .map(|bytes| RunOutput::from_stdout(render_preview(&bytes, color)))
            .map_err(|failure| file_error(&path, failure)),
        Command::Convert(path) => files
            .convert(&path)
            .map(|()| RunOutput::empty())
            .map_err(|failure| file_error(&path, failure)),
    }
}

fn parse_args<I, S>(args: I) -> Result<Command, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut path = None;
    let mut force = false;
    let mut parse_options = true;

    for value in args {
        let argument = value.into();
        if parse_options {
            match argument.to_str() {
                Some("-h" | "-?" | "--help") => return Ok(Command::Help),
                Some("-v" | "--version") => return Ok(Command::Version),
                Some("-f" | "--force") => {
                    force = true;
                    continue;
                }
                Some("--") => {
                    parse_options = false;
                    continue;
                }
                Some(text) if text.starts_with('-') => {
                    return Err(CliError::usage(format!(
                        "parse option {text:?}: unsupported option; use --help for usage"
                    )));
                }
                _ => {}
            }
        }
        if path.replace(PathBuf::from(&argument)).is_some() {
            return Err(CliError::usage(format!(
                "parse operand {argument:?}: expected exactly one FILE; use --help for usage"
            )));
        }
    }

    let path = path.ok_or_else(|| {
        CliError::usage("parse arguments: missing FILE operand; use --help for usage")
    })?;
    Ok(if force {
        Command::Convert(path)
    } else {
        Command::Preview(path)
    })
}

fn help(color: ColorMode) -> String {
    let name = color.paint(WHITE, PROGRAM_NAME);
    format!(
        "{name} v{}\n\
Preview or convert CRLF line endings — https://github.com/queone/rkit\n\
Usage\n\
  {name} [options] [--] FILE\n\
\n\
  Preview FILE and display each CRLF pair as visible \\\\r\\\\n text.\n\
  Use -- before a FILE whose name begins with a dash.\n\
\n\
Options\n\
  -f, --force    Convert CRLF pairs to LF in place\n\
  -v, --version  Print version and exit\n\
  -h, -?, --help Show this help message and exit\n\
  --             End option parsing\n",
        PROGRAM_VERSION
    )
}

fn render_preview(input: &[u8], color: ColorMode) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let marker: &[u8] = if color.enabled() {
        b"\x1b[34m\\r\\n\x1b[0m"
    } else {
        b"\\r\\n"
    };
    let mut start = 0;
    while let Some(offset) = input[start..].windows(2).position(|pair| pair == b"\r\n") {
        let position = start + offset;
        output.extend_from_slice(&input[start..position]);
        output.extend_from_slice(marker);
        output.push(b'\n');
        start = position + 2;
    }
    output.extend_from_slice(&input[start..]);
    output
}

fn convert_bytes(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut start = 0;
    while let Some(offset) = input[start..].windows(2).position(|pair| pair == b"\r\n") {
        let position = start + offset;
        output.extend_from_slice(&input[start..position]);
        output.push(b'\n');
        start = position + 2;
    }
    output.extend_from_slice(&input[start..]);
    output
}

fn file_error(path: &Path, failure: FileFailure) -> CliError {
    match failure {
        FileFailure::BeforeWrite { operation, error } => CliError::runtime(format!(
            "{operation} file {path:?}: {error}; verify the operand names a readable regular file and retry"
        )),
        FileFailure::AfterTruncate { operation, error } => CliError::runtime(format!(
            "{operation} converted file {path:?}: {error}; the file may be partially written; restore it from a backup or source control before retrying"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    enum Scenario {
        Preview(Vec<u8>),
        Converted,
        Failure {
            operation: &'static str,
            after_truncate: bool,
        },
    }

    struct MemoryFiles {
        scenario: Scenario,
    }

    impl FileAccess for MemoryFiles {
        fn preview(&self, _path: &Path) -> Result<Vec<u8>, FileFailure> {
            match &self.scenario {
                Scenario::Preview(bytes) => Ok(bytes.clone()),
                Scenario::Failure {
                    operation,
                    after_truncate,
                } => Err(failure(operation, *after_truncate)),
                Scenario::Converted => unreachable!("preview scenario"),
            }
        }

        fn convert(&self, _path: &Path) -> Result<(), FileFailure> {
            match &self.scenario {
                Scenario::Converted => Ok(()),
                Scenario::Failure {
                    operation,
                    after_truncate,
                } => Err(failure(operation, *after_truncate)),
                Scenario::Preview(_) => unreachable!("conversion scenario"),
            }
        }
    }

    fn failure(operation: &'static str, after_truncate: bool) -> FileFailure {
        let error = io::Error::other("injected failure");
        if after_truncate {
            FileFailure::AfterTruncate { operation, error }
        } else {
            FileFailure::BeforeWrite { operation, error }
        }
    }

    #[test]
    fn parses_options_terminal_flags_and_dash_operand() {
        assert_eq!(
            parse_args(["-f", "file"]).unwrap(),
            Command::Convert(PathBuf::from("file"))
        );
        assert_eq!(
            parse_args(["file", "--force"]).unwrap(),
            Command::Convert(PathBuf::from("file"))
        );
        assert_eq!(
            parse_args(["--", "-file"]).unwrap(),
            Command::Preview(PathBuf::from("-file"))
        );
        assert_eq!(
            parse_args(["file", "--help", "extra"]).unwrap(),
            Command::Help
        );
        assert_eq!(
            parse_args(["file", "--version", "extra"]).unwrap(),
            Command::Version
        );
    }

    #[test]
    fn rejects_invalid_argument_shapes() {
        for args in [
            Vec::<&str>::new(),
            vec!["--bad"],
            vec!["first", "second"],
            vec!["-file"],
        ] {
            let error = parse_args(args).unwrap_err();
            assert_eq!(error.exit_code(), 2);
            assert!(error.message().contains("use --help"));
        }
    }

    #[test]
    fn previews_arbitrary_bytes_with_exact_color_policy() {
        let files = MemoryFiles {
            scenario: Scenario::Preview(b"a\r\nb\nc\rd\xff".to_vec()),
        };
        let plain = run_with(["file"], &files, ColorMode::new(false)).unwrap();
        assert_eq!(plain.stdout(), b"a\\r\\n\nb\nc\rd\xff");
        let colored = run_with(["file"], &files, ColorMode::new(true)).unwrap();
        assert_eq!(colored.stdout(), b"a\x1b[34m\\r\\n\x1b[0m\nb\nc\rd\xff");
    }

    #[test]
    fn colors_each_help_command_name_when_enabled() {
        let files = MemoryFiles {
            scenario: Scenario::Converted,
        };
        let colored = run_with(["--help"], &files, ColorMode::new(true)).unwrap();
        let text = String::from_utf8(colored.stdout().to_vec()).unwrap();
        assert_eq!(text.matches("\x1b[38;5;15mdos2unix\x1b[0m").count(), 2);

        let plain = run_with(["--help"], &files, ColorMode::new(false)).unwrap();
        assert!(!plain.stdout().contains(&b'\x1b'));
    }

    #[test]
    fn converts_byte_pairs_without_utf8_decoding() {
        assert_eq!(
            convert_bytes(b"a\r\nb\nc\rd\xff"),
            b"a\nb\nc\rd\xff".to_vec()
        );
    }

    #[test]
    fn reports_injected_prewrite_operations_without_output() {
        for operation in ["open", "inspect", "read"] {
            let files = MemoryFiles {
                scenario: Scenario::Failure {
                    operation,
                    after_truncate: false,
                },
            };
            let error = run_with(["file"], &files, ColorMode::new(false)).unwrap_err();
            assert_eq!(error.exit_code(), 1);
            assert!(error.message().contains(operation));
            assert!(error.message().contains("retry"));
        }
    }

    #[test]
    fn warns_that_post_truncate_failure_can_damage_file() {
        for operation in ["write", "flush"] {
            let files = MemoryFiles {
                scenario: Scenario::Failure {
                    operation,
                    after_truncate: true,
                },
            };
            let error = run_with(["--force", "file"], &files, ColorMode::new(false)).unwrap_err();
            assert_eq!(error.exit_code(), 1);
            assert!(error.message().contains("partially written"));
            assert!(error.message().contains("backup or source control"));
        }
    }

    #[test]
    fn conversion_success_has_no_output() {
        let files = MemoryFiles {
            scenario: Scenario::Converted,
        };
        let output = run_with(["file", "-f"], &files, ColorMode::new(false)).unwrap();
        assert!(output.stdout().is_empty());
    }
}
