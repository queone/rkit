//! Orientation-aware two-video concatenation through `ffmpeg`.

use crate::color::ColorMode;
use serde_json::Value;
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const PROGRAM_NAME: &str = "vjoin";
const OUTPUT: &str = "merged.mp4";

/// Run vjoin and return its process exit code.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.into().to_string_lossy().into_owned())
        .collect();
    if args.is_empty() {
        write_usage(stdout, version);
        return 0;
    }
    if args.len() == 1 && matches!(args[0].as_str(), "-v" | "--version") {
        let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
        return 0;
    }
    if args.len() == 1 && matches!(args[0].as_str(), "-h" | "-?" | "--help") {
        write_usage(stdout, version);
        return 0;
    }
    if args.iter().any(|arg| arg.starts_with('-')) {
        return fail(
            stderr,
            2,
            format!("unknown flag (see {PROGRAM_NAME} --help)"),
        );
    }
    if args.len() != 2 {
        return fail(
            stderr,
            2,
            format!("expected INPUT1 INPUT2 (see {PROGRAM_NAME} --help)"),
        );
    }
    let tools = SystemTools;
    let filesystem = SystemFileAccess;
    match join_with(&tools, &filesystem, &args[0], &args[1], stdout) {
        Ok(()) => 0,
        Err(error) => fail(stderr, 1, error),
    }
}

trait MediaTools {
    fn probe(&self, path: &str) -> Result<Vec<u8>, String>;
    fn ffmpeg(&self, args: &[String]) -> Result<(), String>;
}

trait FileAccess {
    fn output_exists(&self, path: &Path) -> bool;
}

struct SystemFileAccess;

impl FileAccess for SystemFileAccess {
    fn output_exists(&self, path: &Path) -> bool {
        path.exists() || std::fs::symlink_metadata(path).is_ok()
    }
}

struct SystemTools;

impl MediaTools for SystemTools {
    fn probe(&self, path: &str) -> Result<Vec<u8>, String> {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "stream=codec_type,width,height:stream_tags=rotate:stream_side_data=rotation",
                "-of",
                "json",
                path,
            ])
            .output()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    "ffprobe not found on PATH; install it with: brew install ffmpeg".to_owned()
                } else {
                    format!(
                        "probing input {path}: {error}; verify the file is readable and valid media"
                    )
                }
            })?;
        if !output.status.success() {
            return Err(format!(
                "probing input {path}: ffprobe failed; verify the file is readable and valid media"
            ));
        }
        Ok(output.stdout)
    }

    fn ffmpeg(&self, args: &[String]) -> Result<(), String> {
        let status = Command::new("ffmpeg")
            .args(args)
            .stdin(Stdio::null())
            .status()
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    "ffmpeg not found on PATH; install it with: brew install ffmpeg".to_owned()
                } else {
                    format!("ffmpeg failed: {error}")
                }
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "ffmpeg failed with status {status}; review ffmpeg output and verify both inputs are valid"
            ))
        }
    }
}

fn join_with<W: Write>(
    tools: &impl MediaTools,
    files: &impl FileAccess,
    first: &str,
    second: &str,
    stdout: &mut W,
) -> Result<(), String> {
    if files.output_exists(Path::new(OUTPUT)) {
        return Err(format!(
            "output {OUTPUT} already exists; move or remove it and retry"
        ));
    }
    let first_info = probe(tools, first)?;
    let second_info = probe(tools, second)?;
    tools.ffmpeg(&ffmpeg_args(
        first,
        second,
        first_info.vertical,
        second_info.vertical,
    ))?;
    writeln!(stdout, "Created: {OUTPUT}").map_err(|error| format!("write command output: {error}"))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct MediaInfo {
    vertical: bool,
}

fn probe(tools: &impl MediaTools, path: &str) -> Result<MediaInfo, String> {
    let body = tools.probe(path)?;
    let value: Value = serde_json::from_slice(&body).map_err(|error| {
        format!("parsing ffprobe output for {path}: {error}; verify ffprobe is working correctly")
    })?;
    let streams = value.get("streams").and_then(Value::as_array).ok_or_else(|| {
        format!("parsing ffprobe output for {path}: missing streams; verify ffprobe is working correctly")
    })?;
    let mut video = None;
    let mut has_audio = false;
    for stream in streams {
        match stream.get("codec_type").and_then(Value::as_str) {
            Some("video") if video.is_none() => video = Some(stream),
            Some("audio") => has_audio = true,
            _ => {}
        }
    }
    let video = video.ok_or_else(|| {
        format!("input {path} has no video stream; provide a file containing video and audio")
    })?;
    if !has_audio {
        return Err(format!(
            "input {path} has no audio stream; provide a file containing video and audio"
        ));
    }
    let width = video
        .get("width")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let height = video
        .get("height")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    if width <= 0 || height <= 0 {
        return Err(format!(
            "input {path} has invalid video dimensions; verify the file is valid media"
        ));
    }
    Ok(MediaInfo {
        vertical: is_vertical(width, height, stream_rotation(video)),
    })
}

fn stream_rotation(stream: &Value) -> i64 {
    if let Some(values) = stream.get("side_data_list").and_then(Value::as_array) {
        for value in values {
            if let Some(rotation) = value.get("rotation").and_then(Value::as_f64)
                && rotation != 0.0
            {
                return rotation.round() as i64;
            }
        }
    }
    stream
        .get("tags")
        .and_then(|tags| tags.get("rotate"))
        .and_then(Value::as_str)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_default()
}

fn is_vertical(mut width: i64, mut height: i64, rotation: i64) -> bool {
    let rotation = rotation.rem_euclid(360);
    if rotation == 90 || rotation == 270 {
        std::mem::swap(&mut width, &mut height);
    }
    height > width
}

fn ffmpeg_args(
    first: &str,
    second: &str,
    first_vertical: bool,
    second_vertical: bool,
) -> Vec<String> {
    let graph = [
        video_filter(0, first_vertical),
        video_filter(1, second_vertical),
        "[0:a]aresample=48000[a0]".into(),
        "[1:a]aresample=48000[a1]".into(),
        "[v0][a0][v1][a1]concat=n=2:v=1:a=1[v][a]".into(),
    ]
    .join(";");
    vec![
        "-hide_banner".into(),
        "-n".into(),
        "-i".into(),
        first.into(),
        "-i".into(),
        second.into(),
        "-filter_complex".into(),
        graph,
        "-map".into(),
        "[v]".into(),
        "-map".into(),
        "[a]".into(),
        "-c:v".into(),
        "libx264".into(),
        "-preset".into(),
        "medium".into(),
        "-crf".into(),
        "18".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        "192k".into(),
        "-movflags".into(),
        "+faststart".into(),
        OUTPUT.into(),
    ]
}

fn video_filter(index: usize, vertical: bool) -> String {
    if vertical {
        format!(
            "[{index}:v]split=2[fg{index}][bg{index}];[bg{index}]scale=1920:1080:force_original_aspect_ratio=increase,crop=1920:1080,boxblur=20:10[blur{index}];[fg{index}]scale=1920:1080:force_original_aspect_ratio=decrease[front{index}];[blur{index}][front{index}]overlay=(W-w)/2:(H-h)/2,setsar=1,fps=30[v{index}]"
        )
    } else {
        format!(
            "[{index}:v]scale=1920:1080:force_original_aspect_ratio=decrease,pad=1920:1080:(ow-iw)/2:(oh-ih)/2:black,setsar=1,fps=30[v{index}]"
        )
    }
}

fn write_usage(stdout: &mut impl Write, version: &str) {
    let color = ColorMode::detect_stdout();
    let heading = |text: &str| color.paint("38;5;15", text);
    let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
    let _ = writeln!(
        stdout,
        "Join two videos into one normalized MP4 by driving ffmpeg.\n"
    );
    let _ = writeln!(stdout, "{}", heading("Overview"));
    let _ = writeln!(
        stdout,
        "  vjoin concatenates INPUT1 followed by INPUT2. Vertical clips receive a"
    );
    let _ = writeln!(
        stdout,
        "  blurred background; horizontal and square clips receive black padding.\n"
    );
    let _ = writeln!(stdout, "{}", heading("Usage"));
    let _ = writeln!(stdout, "  vjoin INPUT1 INPUT2\n");
    let _ = writeln!(stdout, "{}", heading("Processing"));
    let _ = writeln!(
        stdout,
        "  Video is normalized to 1920x1080, square pixels, and 30 fps. Audio is"
    );
    let _ = writeln!(
        stdout,
        "  resampled to 48000 Hz. Output uses H.264 CRF 18 and AAC at 192k.\n"
    );
    let _ = writeln!(stdout, "{}", heading("Options"));
    let _ = writeln!(stdout, "  -v, --version          Print version and exit");
    let _ = writeln!(
        stdout,
        "  -h, -?, --help         Show this help message and exit\n"
    );
    let _ = writeln!(stdout, "{}", heading("Notes"));
    let _ = writeln!(
        stdout,
        "  Writes merged.mp4 in the current directory and refuses to overwrite it."
    );
    let _ = writeln!(
        stdout,
        "  Each input must contain at least one video and one audio stream."
    );
    let _ = writeln!(
        stdout,
        "  Requires ffmpeg and ffprobe on PATH (brew install ffmpeg)."
    );
}

fn fail(stderr: &mut impl Write, code: u8, message: String) -> u8 {
    let _ = writeln!(stderr, "{PROGRAM_NAME}: {message}");
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orientation_accounts_for_rotation() {
        assert!(is_vertical(1080, 1920, 0));
        assert!(!is_vertical(1920, 1080, 0));
        assert!(is_vertical(1920, 1080, 90));
        assert!(is_vertical(1920, 1080, -270));
        assert!(!is_vertical(1080, 1080, 0));
    }

    #[test]
    fn filter_graph_covers_both_orientation_branches() {
        let args = ffmpeg_args("first.mp4", "second.mp4", true, false);
        let graph = args.iter().find(|arg| arg.contains("concat=n=2")).unwrap();
        assert!(graph.contains("boxblur=20:10"));
        assert!(graph.contains("pad=1920:1080"));
        assert!(args.contains(&"-n".into()));
        assert!(!args.contains(&"-y".into()));
        assert_eq!(args.last().unwrap(), OUTPUT);
    }

    #[test]
    fn probe_rejects_missing_streams() {
        let value = serde_json::json!({"streams": [{"codec_type": "video", "width": 1920, "height": 1080}]});
        struct Fake(Vec<u8>);
        impl MediaTools for Fake {
            fn probe(&self, _path: &str) -> Result<Vec<u8>, String> {
                Ok(self.0.clone())
            }
            fn ffmpeg(&self, _args: &[String]) -> Result<(), String> {
                Ok(())
            }
        }
        let error = probe(&Fake(value.to_string().into_bytes()), "x.mp4").unwrap_err();
        assert!(error.contains("no audio stream"));
    }
}
