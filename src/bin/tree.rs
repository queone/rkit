use std::io::{self, Write};
use std::process::ExitCode;

const PROGRAM_VERSION: &str = "1.4.0";

fn main() -> ExitCode {
    match rkit::tree::run(std::env::args_os().skip(1), PROGRAM_VERSION) {
        Ok(output) => {
            if let Err(error) = io::stdout().write_all(output.stdout().as_bytes()) {
                eprintln!("write tree output: {error}; verify standard output is writable");
                return ExitCode::from(1);
            }
            if let Err(error) = io::stderr().write_all(output.stderr().as_bytes()) {
                eprintln!("write tree warnings: {error}; verify standard error is writable");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", error.message());
            ExitCode::from(error.exit_code())
        }
    }
}
