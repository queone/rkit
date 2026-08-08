use std::io;
use std::process::ExitCode;

const PROGRAM_VERSION: &str = "0.1.2";

fn main() -> ExitCode {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    ExitCode::from(rkit::attune::run(
        std::env::args_os().skip(1),
        PROGRAM_VERSION,
        &mut stdout,
        &mut stderr,
    ))
}
