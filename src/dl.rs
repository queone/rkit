//! yt-dlp wrapper behavior for the `dl` utility.

use crate::color::ColorMode;
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::Command;

const PROGRAM_NAME: &str = "dl";
const WHITE: &str = "38;5;15";
const GREEN: &str = "1;32";
const RED: &str = "1;31";

/// Runs `dl` and writes its process output to the supplied streams.
///
/// `-v`/`--version` and help are handled before any yt-dlp presence check;
/// this deliberately diverges from Go, which checked yt-dlp before every
/// invocation and printed a second `yt-dlp <version>` line for `-v`.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let color = ColorMode::detect_stdout();

    if args.len() != 1 && args.iter().any(|arg| is_flag(arg, "-v", "--version")) {
        let _ = writeln!(
            stderr,
            "{PROGRAM_NAME}: version flag cannot be combined with other operands"
        );
        return 2;
    }

    if args.len() == 1 {
        if is_flag(&args[0], "-v", "--version") {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            return 0;
        }
        if is_help_flag(&args[0]) {
            let _ = write!(stdout, "{}", usage_text(color));
            return 0;
        }
        if is_flag(&args[0], "-u", "--update") {
            return update_yt_dlp(version, color, stdout, stderr);
        }
    }

    if args.len() != 2 {
        let _ = write!(stdout, "{}", usage_text(color));
        return 1;
    }

    if !yt_dlp_available() {
        print_yt_dlp_missing(color, stdout, stderr);
        return 1;
    }

    download(
        &args[0].to_string_lossy(),
        &args[1].to_string_lossy(),
        color,
        stdout,
        stderr,
    )
}

fn download<W: Write, E: Write>(
    filename: &str,
    url: &str,
    color: ColorMode,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    let filename = normalize_filename(filename);
    if Path::new(&filename).exists() {
        let _ = writeln!(
            stdout,
            "{}",
            color.paint(RED, &format!("File already exists: {filename}"))
        );
        let _ = writeln!(stderr, "{}", color.paint(RED, "Error: file already exists"));
        return 1;
    }

    let _ = writeln!(
        stdout,
        "==> {}",
        color.paint(GREEN, &format!("Downloading to: {filename}"))
    );
    match Command::new("yt-dlp")
        .args(["-o", &filename, "--recode-video", "mp4", url])
        .status()
    {
        Ok(status) if status.success() => {
            let _ = writeln!(
                stdout,
                "{}",
                color.paint(GREEN, "✓ Download completed successfully")
            );
            0
        }
        Ok(status) => {
            let _ = writeln!(
                stderr,
                "{}",
                color.paint(RED, &format!("Error: download failed: {status}"))
            );
            1
        }
        Err(error) => {
            let _ = writeln!(
                stderr,
                "{}",
                color.paint(RED, &format!("Error: download failed: {error}"))
            );
            1
        }
    }
}

fn update_yt_dlp<W: Write, E: Write>(
    version: &str,
    color: ColorMode,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    if !yt_dlp_available() {
        print_yt_dlp_missing(color, stdout, stderr);
        return 1;
    }

    let _ = writeln!(
        stdout,
        "==> {}",
        color.paint(GREEN, "Upgrading yt-dlp to nightly")
    );
    match Command::new("yt-dlp")
        .args(["--update-to", "nightly"])
        .status()
    {
        Ok(status) if status.success() => {}
        Ok(status) => {
            let _ = writeln!(stderr, "{}", color.paint(RED, &format!("Error: {status}")));
            return 1;
        }
        Err(error) => {
            let _ = writeln!(stderr, "{}", color.paint(RED, &format!("Error: {error}")));
            return 1;
        }
    }

    let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
    match yt_dlp_version() {
        Ok(yt_version) => {
            let _ = writeln!(stdout, "yt-dlp {yt_version}");
            0
        }
        Err(error) => {
            let _ = writeln!(stderr, "Error: {error}");
            1
        }
    }
}

fn yt_dlp_version() -> Result<String, String> {
    let output = Command::new("yt-dlp")
        .arg("--version")
        .output()
        .map_err(|error| format!("failed to get yt-dlp version: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "failed to get yt-dlp version: exit status {}",
            output.status
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn print_yt_dlp_missing<W: Write, E: Write>(color: ColorMode, stdout: &mut W, stderr: &mut E) {
    let _ = writeln!(
        stderr,
        "{}\n",
        color.paint(RED, "Error: yt-dlp is not installed or not in PATH")
    );
    let _ = writeln!(stdout, "To install yt-dlp, run:");
    let _ = writeln!(
        stdout,
        "  curl -L https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp \\"
    );
    let _ = writeln!(
        stdout,
        "    -o /usr/local/bin/yt-dlp && chmod a+rx /usr/local/bin/yt-dlp"
    );
    let _ = writeln!(stdout, "\nOr install via package manager:");
    let _ = writeln!(stdout, "  brew install yt-dlp        # macOS");
    let _ = writeln!(stdout, "  pip install yt-dlp         # pip");
}

fn yt_dlp_available() -> bool {
    let Some(path_var) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path_var).any(|dir| is_executable(&dir.join("yt-dlp")))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Appends `.mp4` unless the final path component's extension is already
/// (case-insensitively) `.mp4`.
fn normalize_filename(filename: &str) -> String {
    let base = Path::new(filename)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| filename.to_owned());
    let extension = base.rfind('.').map(|index| base[index..].to_lowercase());
    if extension.as_deref() == Some(".mp4") {
        filename.to_owned()
    } else {
        format!("{filename}.mp4")
    }
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}

fn is_help_flag(value: &OsString) -> bool {
    value == "-?" || value == "-h" || value == "--help"
}

fn usage_text(color: ColorMode) -> String {
    let name = color.paint(WHITE, PROGRAM_NAME);
    let options = color.paint(WHITE, "Options");
    let examples = color.paint(WHITE, "Examples");
    format!(
        "Usage: {name} [OPTIONS] FILENAME \"URL\"\n\
\n\
{options}:\n\
  -u, --update       Upgrade yt-dlp to nightly version\n\
  -v, --version      Show version information\n\
  -?, -h, --help     Show this help message and exit\n\
\n\
{examples}:\n\
  {name} myvideo \"https://youtube.com/watch?v=...\"\n\
  {name} -u\n\
  {name} -v\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_normalization_preserves_existing_mp4_and_appends_otherwise() {
        assert_eq!(normalize_filename("clip"), "clip.mp4");
        assert_eq!(normalize_filename("clip.mkv"), "clip.mkv.mp4");
        assert_eq!(normalize_filename("clip.MP4"), "clip.MP4");
        assert_eq!(normalize_filename("clip.Mp4"), "clip.Mp4");
        assert_eq!(normalize_filename("dir/clip.mkv"), "dir/clip.mkv.mp4");
        assert_eq!(normalize_filename("noext"), "noext.mp4");
    }
}
