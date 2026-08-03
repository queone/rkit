use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

const PROGRAM_VERSION: &str = "1.4.0";

fn execute<I, S, W, E>(args: I, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    match rkit::dos2unix::run(args, PROGRAM_VERSION) {
        Ok(output) => {
            if let Err(error) = stdout.write_all(output.stdout()) {
                let _ = writeln!(
                    stderr,
                    "write command output: {error}; verify standard output is writable and retry"
                );
                return 1;
            }
            0
        }
        Err(error) => {
            if writeln!(stderr, "{}", error.message()).is_err() {
                return 1;
            }
            error.exit_code()
        }
    }
}

fn main() -> ExitCode {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    ExitCode::from(execute(
        std::env::args_os().skip(1),
        &mut stdout,
        &mut stderr,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailingWriter {
        attempts: usize,
    }

    impl Write for FailingWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            self.attempts += 1;
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "injected"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn stdout_failure_reports_once_and_exits_one() {
        let mut stdout = FailingWriter { attempts: 0 };
        let mut stderr = Vec::new();
        let code = execute(["--help"], &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert_eq!(stdout.attempts, 1);
        let diagnostic = String::from_utf8(stderr).unwrap();
        assert!(diagnostic.contains("write command output"));
        assert!(diagnostic.contains("writable and retry"));
    }

    #[test]
    fn stderr_failure_is_not_retried_and_exits_one() {
        let mut stdout = Vec::new();
        let mut stderr = FailingWriter { attempts: 0 };
        let code = execute(Vec::<&str>::new(), &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert_eq!(stderr.attempts, 1);
    }
}
