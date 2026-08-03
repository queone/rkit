//! Homebrew update, upgrade, cask-upgrade, and cleanup behavior.

use crate::color::ColorMode;
use std::ffi::OsString;
use std::io;
use std::process::{Command, Output, Stdio};

const PROGRAM_NAME: &str = "brew-update";
const GREEN: &str = "38;5;46";
const RED: &str = "38;5;124";
const RECOVERY: &str = "verify that Homebrew is installed and brew is on PATH, then retry";

/// A Homebrew workflow failure with a process exit code.
#[derive(Debug)]
pub struct CliError {
    message: String,
}

impl CliError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Returns the diagnostic text intended for standard error.
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Runs the Homebrew workflow, streaming child command output to the process
/// standard streams.
pub fn run<I, S>(args: I, version: &str) -> Result<(), CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if is_version_request(&args) {
        println!("{PROGRAM_NAME} v{version}");
        return Ok(());
    }

    let color = ColorMode::detect_stdout();
    println!("{PROGRAM_NAME} {version}\n");

    run_streamed_command(&["update"], "brew update", color)
        .map_err(|error| workflow_error("Error during brew update", error, color))?;
    run_streamed_command(&["upgrade"], "brew upgrade", color)
        .map_err(|error| workflow_error("Error during brew upgrade", error, color))?;

    let casks =
        list_casks().map_err(|error| workflow_error("Error during cask upgrade", error, color))?;
    if !casks.is_empty() {
        let mut command_args = vec![OsString::from("upgrade")];
        command_args.extend(casks);
        let display = display_command(&command_args);
        run_streamed_command_os(&command_args, &display, color)
            .map_err(|error| workflow_error("Error during cask upgrade", error, color))?;
    }

    run_streamed_command(&["cleanup", "-s"], "brew cleanup -s", color)
        .map_err(|error| workflow_error("Error during brew cleanup -s", error, color))?;

    println!(
        "\n{}",
        color.paint(GREEN, "✓ All updates completed successfully")
    );
    Ok(())
}

fn is_version_request(args: &[OsString]) -> bool {
    args.len() == 1 && matches!(args[0].to_str(), Some("-v" | "--version"))
}

fn list_casks() -> Result<Vec<OsString>, CommandError> {
    let output = Command::new("brew")
        .args(["list", "--cask"])
        .output()
        .map_err(CommandError::spawn)?;
    if !output.status.success() {
        return Err(CommandError::status(output));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|cask| !cask.is_empty())
        .map(OsString::from)
        .collect())
}

fn run_streamed_command(
    args: &[&str],
    display: &str,
    color: ColorMode,
) -> Result<(), CommandError> {
    let args = args.iter().map(OsString::from).collect::<Vec<_>>();
    run_streamed_command_os(&args, display, color)
}

fn run_streamed_command_os(
    args: &[OsString],
    display: &str,
    color: ColorMode,
) -> Result<(), CommandError> {
    println!("==> {}", color.paint(GREEN, display));
    let status = Command::new("brew")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(CommandError::spawn)?;
    if status.success() {
        Ok(())
    } else {
        Err(CommandError::status_code(status.code()))
    }
}

fn display_command(args: &[OsString]) -> String {
    let mut display = String::from("brew");
    for arg in args {
        display.push(' ');
        display.push_str(&arg.to_string_lossy());
    }
    display
}

fn workflow_error(context: &str, error: CommandError, color: ColorMode) -> CliError {
    CliError::new(format!(
        "{}: {}; {}.",
        color.paint(RED, context),
        error,
        RECOVERY
    ))
}

#[derive(Debug)]
enum CommandError {
    Spawn(io::Error),
    Status(Output),
    StatusCode(Option<i32>),
}

impl CommandError {
    fn spawn(error: io::Error) -> Self {
        Self::Spawn(error)
    }

    fn status(output: Output) -> Self {
        Self::Status(output)
    }

    fn status_code(code: Option<i32>) -> Self {
        Self::StatusCode(code)
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(error) => write!(formatter, "could not execute brew: {error}"),
            Self::Status(output) => write!(formatter, "brew exited with {}", status_text(output)),
            Self::StatusCode(code) => write!(
                formatter,
                "brew exited with {}",
                code.map_or_else(|| "a signal".to_owned(), |code| format!("status {code}"))
            ),
        }
    }
}

fn status_text(output: &Output) -> String {
    output
        .status
        .code()
        .map_or_else(|| "a signal".to_owned(), |code| format!("status {code}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_request_requires_one_exact_argument() {
        assert!(is_version_request(&[OsString::from("-v")]));
        assert!(is_version_request(&[OsString::from("--version")]));
        assert!(!is_version_request(&[]));
        assert!(!is_version_request(&[
            OsString::from("-v"),
            OsString::from("extra")
        ]));
        assert!(!is_version_request(&[OsString::from("--help")]));
    }

    #[test]
    fn display_command_preserves_argument_boundaries() {
        let args = [OsString::from("upgrade"), OsString::from("alpha")];
        assert_eq!(display_command(&args), "brew upgrade alpha");
    }

    #[test]
    fn color_contract_uses_existing_palette() {
        let color = ColorMode::new(true);
        assert_eq!(color.paint(GREEN, "command"), "\x1b[38;5;46mcommand\x1b[0m");
        assert_eq!(color.paint(RED, "error"), "\x1b[38;5;124merror\x1b[0m");
    }
}
