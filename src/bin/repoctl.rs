use std::io::{self, Write};
use std::process::ExitCode;

const PROGRAM_VERSION: &str = "0.3.0";

fn main() -> ExitCode {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    match rkit::repoctl::run(
        std::env::args_os().skip(1),
        PROGRAM_VERSION,
        &mut stdout,
        &mut stderr,
    ) {
        Ok(exit_code) => ExitCode::from(exit_code),
        Err(error) => {
            let _ = writeln!(stderr, "{}", error.message());
            ExitCode::from(error.exit_code())
        }
    }
}
