use std::io;
use std::process::ExitCode;

const PROGRAM_VERSION: &str = "1.5.0";

fn main() -> ExitCode {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    ExitCode::from(rkit::rn::run(
        std::env::args_os().skip(1),
        PROGRAM_VERSION,
        &mut stdout,
        &mut stderr,
    ))
}
