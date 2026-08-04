use std::io::{self, BufReader, Write};
use std::process::ExitCode;

const PROGRAM_VERSION: &str = "2.0.0";

fn main() -> ExitCode {
    let stdin = io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let code = rkit::certgen::run(
        std::env::args().skip(1),
        PROGRAM_VERSION,
        &mut input,
        &mut stdout,
        &mut stderr,
    );
    let _ = stdout.flush();
    let _ = stderr.flush();
    ExitCode::from(code)
}
