use std::io::{self, Write};
use std::process::ExitCode;

const PROGRAM_VERSION: &str = "2.0.0";

fn main() -> ExitCode {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let code = rkit::mdview::run(
        std::env::args_os().skip(1),
        PROGRAM_VERSION,
        &mut stdout,
        &mut stderr,
    );
    let _ = stdout.flush();
    let _ = stderr.flush();
    ExitCode::from(code)
}
