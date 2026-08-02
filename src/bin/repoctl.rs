use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    match rkit::repoctl::run(std::env::args_os().skip(1)) {
        Ok(output) => {
            if stdout.write_all(output.stdout().as_bytes()).is_err() {
                let _ = writeln!(
                    stderr,
                    "write repoctl output: verify standard output is writable and retry"
                );
                return ExitCode::from(1);
            }
            if stderr.write_all(output.stderr().as_bytes()).is_err() {
                return ExitCode::from(1);
            }
            ExitCode::from(output.exit_code())
        }
        Err(error) => {
            let _ = writeln!(stderr, "{}", error.message());
            ExitCode::from(error.exit_code())
        }
    }
}
