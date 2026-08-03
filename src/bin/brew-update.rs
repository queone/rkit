use std::io::{self, Write};
use std::process::ExitCode;

const PROGRAM_VERSION: &str = "1.3.5";

fn main() -> ExitCode {
    match rkit::brew_update::run(std::env::args_os().skip(1), PROGRAM_VERSION) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let mut stderr = io::stderr();
            if writeln!(stderr, "{}", error.message()).is_err() {
                return ExitCode::from(1);
            }
            ExitCode::from(1)
        }
    }
}
