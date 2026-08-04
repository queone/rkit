use std::io::{self, BufReader};
use std::process::ExitCode;

const PROGRAM_VERSION: &str = "2.0.0";

fn main() -> ExitCode {
    let stdin = io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    ExitCode::from(rkit::rnlower::run(
        std::env::args_os().skip(1),
        PROGRAM_VERSION,
        &mut input,
        &mut stdout,
        &mut stderr,
    ))
}
