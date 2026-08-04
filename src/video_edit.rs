//! Shared video-section editing behavior for `vkeep` and `vdrop`.

use crate::color::ColorMode;
use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

const KEEP_NAME: &str = "vkeep";
const DROP_NAME: &str = "vdrop";

/// Run the `vkeep` command and return its process exit code.
pub fn run_keep<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    run(false, args, version, stdout, stderr)
}

/// Run the `vdrop` command and return its process exit code.
pub fn run_drop<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    run(true, args, version, stdout, stderr)
}

fn run<I, S, W, E>(is_drop: bool, args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let name = if is_drop { DROP_NAME } else { KEEP_NAME };
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.into().to_string_lossy().into_owned())
        .collect();
    if args.is_empty() {
        write_usage(stdout, name, version);
        return 0;
    }
    if args.len() == 1 && matches!(args[0].as_str(), "-v" | "--version") {
        let _ = writeln!(stdout, "{name} v{version}");
        return 0;
    }
    if args.len() == 1 && matches!(args[0].as_str(), "-h" | "-?" | "--help") {
        write_usage(stdout, name, version);
        return 0;
    }

    let parsed = match parse_args(is_drop, &args) {
        Ok(parsed) => parsed,
        Err(message) => return fail(stderr, name, 2, message),
    };
    let tools = SystemTools;
    let filesystem = SystemFileAccess;
    if let Err(message) = execute(is_drop, parsed, &tools, &filesystem, stdout) {
        return fail(stderr, name, 1, message);
    }
    0
}

#[derive(Debug, PartialEq)]
struct EditArgs {
    accurate: bool,
    crossfade: f64,
    start: String,
    end: String,
    input: PathBuf,
}

fn parse_args(is_drop: bool, args: &[String]) -> Result<EditArgs, String> {
    let mut accurate = false;
    let mut crossfade = 0.0;
    let mut positional = Vec::new();
    for arg in args {
        match arg.as_str() {
            "-a" | "--accurate" => accurate = true,
            "-x" | "--crossfade" if is_drop => crossfade = 0.5,
            "-x" | "--crossfade" => {
                return Err("--crossfade applies only to vdrop; vkeep keeps a single section and has no join to smooth".into())
            }
            value if is_drop && (value.starts_with("--crossfade=") || value.starts_with("-x=")) => {
                let value = value.split_once('=').map(|(_, value)| value).unwrap_or_default();
                crossfade = value
                    .parse::<f64>()
                    .map_err(|_| format!("invalid crossfade duration {value:?} (see vdrop --help)"))?;
            }
            value if value.starts_with('-') => {
                return Err(format!("unknown flag {value:?} (see {} --help)", if is_drop { DROP_NAME } else { KEEP_NAME }));
            }
            value => positional.push(value.to_owned()),
        }
    }
    let (start, end, input) = match positional.as_slice() {
        [start, input] => (start.clone(), "end".to_owned(), PathBuf::from(input)),
        [start, end, input] => (start.clone(), end.clone(), PathBuf::from(input)),
        _ => {
            return Err(format!(
                "expected START [END] FILE (see {} --help)",
                if is_drop { DROP_NAME } else { KEEP_NAME }
            ));
        }
    };
    Ok(EditArgs {
        accurate,
        crossfade,
        start,
        end,
        input,
    })
}

trait MediaTools {
    fn available(&self) -> Result<(), String>;
    fn duration(&self, path: &Path) -> Result<f64, String>;
    fn ffmpeg(&self, args: &[String]) -> Result<(), String>;
}

trait FileAccess {
    fn metadata(&self, path: &Path) -> std::io::Result<std::fs::Metadata>;
    fn output_exists(&self, path: &Path) -> bool;
    fn copy(&self, source: &Path, destination: &Path) -> std::io::Result<u64>;
    fn write(&self, path: &Path, body: &[u8]) -> std::io::Result<()>;
    fn create_dir(&self, path: &Path) -> std::io::Result<()>;
    fn remove_dir_all(&self, path: &Path);
}

struct SystemFileAccess;

impl FileAccess for SystemFileAccess {
    fn metadata(&self, path: &Path) -> std::io::Result<std::fs::Metadata> {
        fs::metadata(path)
    }

    fn output_exists(&self, path: &Path) -> bool {
        path.exists() || fs::symlink_metadata(path).is_ok()
    }

    fn copy(&self, source: &Path, destination: &Path) -> std::io::Result<u64> {
        fs::copy(source, destination)
    }

    fn write(&self, path: &Path, body: &[u8]) -> std::io::Result<()> {
        fs::write(path, body)
    }

    fn create_dir(&self, path: &Path) -> std::io::Result<()> {
        fs::create_dir(path)
    }

    fn remove_dir_all(&self, path: &Path) {
        let _ = fs::remove_dir_all(path);
    }
}

struct SystemTools;

impl MediaTools for SystemTools {
    fn available(&self) -> Result<(), String> {
        for tool in ["ffmpeg", "ffprobe"] {
            let result = Command::new(tool)
                .arg("-version")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            if result.is_err() {
                return Err(format!(
                    "{tool} not found on PATH — install it with: brew install ffmpeg"
                ));
            }
        }
        Ok(())
    }

    fn duration(&self, path: &Path) -> Result<f64, String> {
        let output = Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path)
            .output()
            .map_err(|error| format!("probing {path:?}: {error} (is it a valid video file?)"))?;
        if !output.status.success() {
            return Err(format!(
                "probing {path:?}: ffprobe failed (is it a valid video file?)"
            ));
        }
        let raw = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        raw.parse::<f64>()
            .map_err(|_| format!("probing {path:?}: could not parse duration {raw:?}"))
    }

    fn ffmpeg(&self, args: &[String]) -> Result<(), String> {
        let status = Command::new("ffmpeg")
            .args(args)
            .status()
            .map_err(|error| format!("ffmpeg failed: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "ffmpeg failed with status {status}; review ffmpeg output"
            ))
        }
    }
}

fn execute<W: Write>(
    is_drop: bool,
    args: EditArgs,
    tools: &impl MediaTools,
    files: &impl FileAccess,
    stdout: &mut W,
) -> Result<(), String> {
    tools.available()?;
    let metadata = files
        .metadata(&args.input)
        .map_err(|_| format!("input {:?}: not a readable file", args.input))?;
    if !metadata.is_file() {
        return Err(format!("input {:?}: not a readable file", args.input));
    }
    let duration = tools.duration(&args.input)?;
    let start = parse_and_gate(&args.start, duration)?;
    let end = resolve_end(&args.end, duration)?;
    if is_drop {
        validate_cut(start, end, duration)?;
    } else {
        validate_clip(start, end, duration)?;
    }
    if args.crossfade > 0.0 {
        validate_crossfade(start, end, duration, args.crossfade)?;
    }
    let output = derive_output_name(&args.input);
    if files.output_exists(&output) {
        return Err(format!(
            "output {:?} already exists; refusing to overwrite",
            output
        ));
    }

    let mut temp = None;
    let plan = if !is_drop {
        keep_plan(start, end, args.accurate, &args.input, &output, duration)
    } else if args.crossfade > 0.0 {
        Plan::single(vec![
            "-i".into(),
            args.input.to_string_lossy().into_owned(),
            "-filter_complex".into(),
            crossfade_filter(start, end, args.crossfade),
            "-map".into(),
            "[v]".into(),
            "-map".into(),
            "[a]".into(),
            output.to_string_lossy().into_owned(),
        ])
    } else {
        let interior = start != 0 && end < duration as i64;
        let temp_dir = if interior && !args.accurate {
            let path = create_temp_dir(files)?;
            temp = Some(path.clone());
            path
        } else {
            PathBuf::new()
        };
        cut_plan(
            start,
            end,
            args.accurate,
            &args.input,
            &output,
            &temp_dir,
            duration,
        )
    };

    let result: Result<(), String> = (|| {
        if plan.commands.is_empty() {
            files
                .copy(&args.input, &output)
                .map_err(|error| format!("copying {:?} to {:?}: {error}", args.input, output))?;
        } else {
            if let Some((path, body)) = plan.concat {
                files
                    .write(&path, body.as_bytes())
                    .map_err(|error| format!("writing concat list: {error}"))?;
            }
            for command in &plan.commands {
                tools.ffmpeg(command)?;
            }
        }
        Ok(())
    })();
    if let Some(path) = temp {
        files.remove_dir_all(&path);
    }
    result?;
    print_summary(tools, files, &args.input, &output, stdout)
}

#[derive(Debug, Default, Eq, PartialEq)]
struct Plan {
    commands: Vec<Vec<String>>,
    concat: Option<(PathBuf, String)>,
}

impl Plan {
    fn single(command: Vec<String>) -> Self {
        Self {
            commands: vec![command],
            concat: None,
        }
    }
}

fn keep_plan(
    start: i64,
    end: i64,
    accurate: bool,
    input: &Path,
    output: &Path,
    duration: f64,
) -> Plan {
    let input = input.to_string_lossy().into_owned();
    let output = output.to_string_lossy().into_owned();
    let start_text = start.to_string();
    let end_text = end.to_string();
    let length = (end - start).to_string();
    let full = end >= duration as i64;
    let command = match (start, full, accurate) {
        (0, true, _) => return Plan::default(),
        (0, false, true) => vec![
            "-i", &input, "-to", &end_text, "-c:v", "libx264", "-c:a", "aac", &output,
        ],
        (0, false, false) => vec!["-i", &input, "-to", &end_text, "-c", "copy", &output],
        (_, true, true) => vec![
            "-i",
            &input,
            "-ss",
            &start_text,
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            &output,
        ],
        (_, true, false) => vec!["-ss", &start_text, "-i", &input, "-c", "copy", &output],
        (_, false, true) => vec![
            "-i",
            &input,
            "-ss",
            &start_text,
            "-t",
            &length,
            "-c:v",
            "libx264",
            "-c:a",
            "aac",
            &output,
        ],
        (_, false, false) => vec![
            "-ss",
            &start_text,
            "-i",
            &input,
            "-t",
            &length,
            "-c",
            "copy",
            &output,
        ],
    };
    Plan::single(command.into_iter().map(str::to_owned).collect())
}

fn cut_plan(
    start: i64,
    end: i64,
    accurate: bool,
    input: &Path,
    output: &Path,
    temp: &Path,
    duration: f64,
) -> Plan {
    if start == 0 {
        return keep_plan(end, duration as i64, accurate, input, output, duration);
    }
    if end >= duration as i64 {
        return keep_plan(0, start, accurate, input, output, duration);
    }
    let input = input.to_string_lossy();
    let output = output.to_string_lossy();
    if accurate {
        return Plan::single(vec![
            "-i".into(),
            input.into_owned(),
            "-filter_complex".into(),
            cut_filter(start, end),
            "-map".into(),
            "[v]".into(),
            "-map".into(),
            "[a]".into(),
            output.into_owned(),
        ]);
    }
    let ext = input
        .rsplit_once('.')
        .map(|(_, ext)| format!(".{ext}"))
        .unwrap_or_default();
    let seg1 = temp.join(format!("seg0{ext}"));
    let seg2 = temp.join(format!("seg1{ext}"));
    let list = temp.join("concat.txt");
    let seg1s = seg1.to_string_lossy().into_owned();
    let seg2s = seg2.to_string_lossy().into_owned();
    let lists = list.to_string_lossy().into_owned();
    Plan {
        commands: vec![
            vec![
                "-i".into(),
                input.clone().into_owned(),
                "-to".into(),
                start.to_string(),
                "-c".into(),
                "copy".into(),
                seg1s.clone(),
            ],
            vec![
                "-ss".into(),
                end.to_string(),
                "-i".into(),
                input.into_owned(),
                "-c".into(),
                "copy".into(),
                seg2s.clone(),
            ],
            vec![
                "-f".into(),
                "concat".into(),
                "-safe".into(),
                "0".into(),
                "-i".into(),
                lists.clone(),
                "-c".into(),
                "copy".into(),
                output.into_owned(),
            ],
        ],
        concat: Some((list, format!("file '{seg1s}'\nfile '{seg2s}'\n"))),
    }
}

fn parse_offset(value: &str) -> Result<(i64, usize), String> {
    let parts: Vec<&str> = value.split(':').collect();
    if !(1..=3).contains(&parts.len()) || parts.iter().any(|part| part.is_empty()) {
        return Err(format!(
            "invalid timestamp {value:?}: use SS, MM:SS, or HH:MM:SS"
        ));
    }
    let mut nums = Vec::new();
    for part in parts {
        let number = part.parse::<i64>().map_err(|_| {
            format!("invalid timestamp {value:?}: fields must be non-negative integers")
        })?;
        if number < 0 {
            return Err(format!(
                "invalid timestamp {value:?}: fields must be non-negative integers"
            ));
        }
        nums.push(number);
    }
    if nums.len() == 2 && nums[1] > 59 || nums.len() == 3 && (nums[1] > 59 || nums[2] > 59) {
        return Err(format!(
            "invalid timestamp {value:?}: minutes and seconds must be 00-59"
        ));
    }
    let seconds = match nums.as_slice() {
        [seconds] => *seconds,
        [minutes, seconds] => minutes * 60 + seconds,
        [hours, minutes, seconds] => hours * 3600 + minutes * 60 + seconds,
        _ => unreachable!(),
    };
    Ok((seconds, nums.len()))
}

fn parse_and_gate(value: &str, duration: f64) -> Result<i64, String> {
    let (seconds, components) = parse_offset(value)?;
    if components == 3 && duration <= 3600.0 {
        return Err(format!(
            "{value:?}: HH:MM:SS form is only allowed when the source is longer than one hour"
        ));
    }
    Ok(seconds)
}

fn resolve_end(value: &str, duration: f64) -> Result<i64, String> {
    if value == "end" {
        Ok(duration as i64)
    } else {
        parse_and_gate(value, duration)
    }
}

fn validate_clip(start: i64, end: i64, duration: f64) -> Result<(), String> {
    if start < 0 || start >= end || end as f64 > duration {
        return Err(format!(
            "require 0 <= start < end <= source duration {}",
            format_duration(duration as i64)
        ));
    }
    Ok(())
}

fn validate_cut(start: i64, end: i64, duration: f64) -> Result<(), String> {
    validate_clip(start, end, duration)?;
    if start == 0 && end >= duration as i64 {
        return Err("cut would remove the entire video".into());
    }
    Ok(())
}

fn validate_crossfade(start: i64, end: i64, duration: f64, crossfade: f64) -> Result<(), String> {
    if start == 0 || end >= duration as i64 {
        return Err(
            "crossfade requires an interior drop — START and END must both fall inside the source"
                .into(),
        );
    }
    if crossfade <= 0.0 || !crossfade.is_finite() {
        return Err("crossfade duration must be greater than zero".into());
    }
    if crossfade > start as f64 {
        return Err(format!(
            "crossfade duration {crossfade}s exceeds the {start}s kept before the drop"
        ));
    }
    if crossfade > duration - end as f64 {
        return Err(format!(
            "crossfade duration {crossfade}s exceeds the {}s kept after the drop",
            duration as i64 - end
        ));
    }
    Ok(())
}

fn derive_output_name(input: &Path) -> PathBuf {
    let file = input.file_name().unwrap_or_default().to_string_lossy();
    let extension = Path::new(file.as_ref())
        .extension()
        .map(|value| format!(".{}", value.to_string_lossy()))
        .unwrap_or_default();
    let stem = file.strip_suffix(&extension).unwrap_or(file.as_ref());
    let split = stem.len()
        - stem
            .trim_end_matches(|character: char| character.is_ascii_digit())
            .len();
    let name = if split == 0 {
        format!("{stem}_1{extension}")
    } else {
        let index = stem.len() - split;
        format!("{}_{}{}", &stem[..index], &stem[index..], extension)
    };
    input.parent().unwrap_or_else(|| Path::new(".")).join(name)
}

fn create_temp_dir(files: &impl FileAccess) -> Result<PathBuf, String> {
    let base = std::env::temp_dir();
    for attempt in 0..100 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let candidate = base.join(format!("vedit-{}-{now}-{attempt}", std::process::id()));
        match files.create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("creating temp dir: {error}")),
        }
    }
    Err("creating temp dir: exhausted unique names".into())
}

fn cut_filter(start: i64, end: i64) -> String {
    format!(
        "[0:v]trim=0:{start},setpts=PTS-STARTPTS[v0];[0:a]atrim=0:{start},asetpts=PTS-STARTPTS[a0];[0:v]trim=start={end},setpts=PTS-STARTPTS[v1];[0:a]atrim=start={end},asetpts=PTS-STARTPTS[a1];[v0][a0][v1][a1]concat=n=2:v=1:a=1[v][a]"
    )
}

fn crossfade_filter(start: i64, end: i64, duration: f64) -> String {
    let offset = start as f64 - duration;
    format!(
        "[0:v]trim=0:{start},setpts=PTS-STARTPTS[v0];[0:v]trim=start={end},setpts=PTS-STARTPTS[v1];[v0][v1]xfade=transition=fade:duration={duration}:offset={offset}[v];[0:a]atrim=0:{start},asetpts=PTS-STARTPTS[a0];[0:a]atrim=start={end},asetpts=PTS-STARTPTS[a1];[a0][a1]acrossfade=d={duration}[a]"
    )
}

fn print_summary<W: Write>(
    tools: &impl MediaTools,
    files: &impl FileAccess,
    input: &Path,
    output: &Path,
    stdout: &mut W,
) -> Result<(), String> {
    let input_stat = summary_stat(tools, files, "input", input)?;
    let output_stat = summary_stat(tools, files, "output", output)?;
    let rows = [input_stat, output_stat];
    let headers = ["FILE", "NAME", "SIZE", "DURATION"];
    let cells: Vec<[String; 4]> = rows
        .map(|row| [row.0, row.1, format_bytes(row.2), format_duration(row.3)])
        .to_vec();
    let widths = (0..4)
        .map(|column| {
            std::iter::once(headers[column].len())
                .chain(cells.iter().map(|row| row[column].len()))
                .max()
                .unwrap_or(0)
        })
        .collect::<Vec<_>>();
    let color = ColorMode::detect_stdout();
    for (column, header) in headers.iter().enumerate() {
        if column > 0 {
            write!(stdout, "  ").map_err(|error| error.to_string())?;
        }
        let value = if column == 2 || column == 3 {
            format!("{header:>width$}", width = widths[column])
        } else {
            format!("{header:<width$}", width = widths[column])
        };
        write!(stdout, "{}", color.paint("38;5;15", &value)).map_err(|error| error.to_string())?;
    }
    writeln!(stdout).map_err(|error| error.to_string())?;
    for row in cells {
        for (column, value) in row.iter().enumerate() {
            if column > 0 {
                write!(stdout, "  ").map_err(|error| error.to_string())?;
            }
            if column == 2 || column == 3 {
                write!(stdout, "{value:>width$}", width = widths[column])
                    .map_err(|error| error.to_string())?;
            } else {
                write!(stdout, "{value:<width$}", width = widths[column])
                    .map_err(|error| error.to_string())?;
            }
        }
        writeln!(stdout).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn summary_stat(
    tools: &impl MediaTools,
    files: &impl FileAccess,
    label: &str,
    path: &Path,
) -> Result<(String, String, i64, i64), String> {
    let size = files
        .metadata(path)
        .map_err(|error| format!("reading {path:?}: {error}"))?
        .len() as i64;
    let duration = tools.duration(path)?.round() as i64;
    Ok((
        label.into(),
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        size,
        duration,
    ))
}

fn format_bytes(value: i64) -> String {
    let raw = value.to_string();
    let (sign, digits) = raw
        .strip_prefix('-')
        .map_or(("", raw.as_str()), |value| ("-", value));
    let mut output = String::new();
    for (index, character) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            output.push(',');
        }
        output.push(character);
    }
    format!("{sign}{output}")
}

fn format_duration(total: i64) -> String {
    let total = total.max(0);
    format!(
        "{:02}:{:02}:{:02}",
        total / 3600,
        (total % 3600) / 60,
        total % 60
    )
}

fn write_usage(stdout: &mut impl Write, invoked: &str, version: &str) {
    let color = ColorMode::detect_stdout();
    let heading = |text: &str| color.paint("38;5;15", text);
    let rows = [
        ("Copy the whole file", "vkeep 0 FILE"),
        ("Keep from beginning to 1:00", "vkeep 0 1:00 FILE"),
        ("Drop from beginning to 1:00", "vdrop 0 1:00 FILE"),
        ("Keep from 1:00 to the end", "vkeep 1:00 FILE"),
        ("Drop from 1:00 to the end", "vdrop 1:00 FILE"),
        ("Keep only the middle 1:00..8:31", "vkeep 1:00 8:31 FILE"),
        ("Drop only the middle 1:00..8:31", "vdrop 1:00 8:31 FILE"),
    ];
    let _ = writeln!(stdout, "{invoked} v{version}");
    let _ = writeln!(
        stdout,
        "Keep or drop a section of a video by driving ffmpeg.\n"
    );
    let _ = writeln!(stdout, "{}", heading("Overview"));
    let _ = writeln!(
        stdout,
        "  vkeep keeps the part you want. vdrop removes the part you don't want"
    );
    let _ = writeln!(
        stdout,
        "  (and joins the remainder). They are counterparts — every result is"
    );
    let _ = writeln!(
        stdout,
        "  reachable from either, but one form is usually shorter.\n"
    );
    let _ = writeln!(stdout, "{}", heading("Usage"));
    let _ = writeln!(
        stdout,
        "  vkeep START [END] [-a] <input>     keep START..END"
    );
    let _ = writeln!(
        stdout,
        "  vdrop START [END] [-a] <input>     drop START..END, join the rest\n"
    );
    let _ = writeln!(stdout, "{}", heading("Timestamps"));
    let _ = writeln!(
        stdout,
        "  MM:SS by default (8:31); a bare integer is whole seconds (90); HH:MM:SS"
    );
    let _ = writeln!(
        stdout,
        "  only when the source is longer than one hour. END is optional — omit it"
    );
    let _ = writeln!(
        stdout,
        "  (or pass the literal 'end') to reach the source end.\n"
    );
    let _ = writeln!(stdout, "{}", heading("Cheatsheet"));
    let _ = writeln!(stdout, "  {:<34}Use this", "What you want");
    for (goal, command) in rows {
        let command = if command.starts_with(&format!("{invoked} ")) {
            heading(command)
        } else {
            command.to_owned()
        };
        let _ = writeln!(stdout, "  {goal:<34}{command}");
    }
    let _ = writeln!(stdout, "\n{}", heading("Options"));
    let _ = writeln!(
        stdout,
        "  -a, --accurate         Frame-accurate re-encode (default: fast keyframe copy)"
    );
    let _ = writeln!(
        stdout,
        "  -x, --crossfade[=SECS] Dissolve the interior join (vdrop only; re-encodes;"
    );
    let _ = writeln!(stdout, "                         default 0.5s)");
    let _ = writeln!(stdout, "  -v, --version          Print version and exit");
    let _ = writeln!(
        stdout,
        "  -h, -?, --help         Show this help message and exit\n"
    );
    let _ = writeln!(stdout, "{}", heading("Notes"));
    let _ = writeln!(
        stdout,
        "  vkeep 0 FILE copies the whole file. vdrop has no whole-file form —"
    );
    let _ = writeln!(
        stdout,
        "  dropping 0..end would remove everything, which vdrop refuses."
    );
    let _ = writeln!(
        stdout,
        "  A vdrop crossfade (-x) overlaps SECS seconds, so the output is that much"
    );
    let _ = writeln!(stdout, "  shorter than a hard cut.");
    let _ = writeln!(
        stdout,
        "  Requires ffmpeg and ffprobe on PATH (brew install ffmpeg)."
    );
}

fn fail(stderr: &mut impl Write, name: &str, code: u8, message: String) -> u8 {
    let _ = writeln!(stderr, "{name}: {message}");
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_offsets_and_duration_gate() {
        assert_eq!(parse_offset("8:31").unwrap(), (511, 2));
        assert_eq!(parse_offset("90").unwrap(), (90, 1));
        assert_eq!(parse_offset("1:08:31").unwrap(), (4111, 3));
        assert!(parse_offset("8:61").is_err());
        assert!(parse_and_gate("1:00:00", 3600.0).is_err());
        assert_eq!(parse_and_gate("1:00:00", 3601.0).unwrap(), 3600);
    }

    #[test]
    fn plans_cover_keep_and_interior_drop() {
        let input = Path::new("in.mp4");
        let output = Path::new("in_1.mp4");
        assert!(
            keep_plan(0, 600, false, input, output, 600.0)
                .commands
                .is_empty()
        );
        assert_eq!(
            keep_plan(60, 511, false, input, output, 600.0).commands[0],
            vec![
                "-ss", "60", "-i", "in.mp4", "-t", "451", "-c", "copy", "in_1.mp4"
            ]
        );
        let plan = cut_plan(60, 300, false, input, output, Path::new("/tmp/vt"), 600.0);
        assert_eq!(plan.commands.len(), 3);
        assert!(plan.concat.unwrap().1.contains("seg0.mp4"));
    }

    #[test]
    fn names_and_filters_match_contract() {
        assert_eq!(
            derive_output_name(Path::new("SOURCE1.mp4")),
            PathBuf::from("SOURCE_1.mp4")
        );
        assert_eq!(
            derive_output_name(Path::new("clip.mp4")),
            PathBuf::from("clip_1.mp4")
        );
        assert!(cut_filter(60, 300).contains("concat=n=2:v=1:a=1"));
        assert!(crossfade_filter(60, 300, 0.5).contains("offset=59.5"));
    }

    #[test]
    fn summary_helpers_format_values() {
        assert_eq!(format_bytes(1_234_567), "1,234,567");
        assert_eq!(format_duration(4111), "01:08:31");
        assert!(validate_cut(0, 600, 600.0).is_err());
        assert!(validate_crossfade(0, 300, 600.0, 1.0).is_err());
    }

    struct FakeTools;

    impl MediaTools for FakeTools {
        fn available(&self) -> Result<(), String> {
            Ok(())
        }

        fn duration(&self, _path: &Path) -> Result<f64, String> {
            Ok(600.0)
        }

        fn ffmpeg(&self, args: &[String]) -> Result<(), String> {
            let output = Path::new(args.last().ok_or("missing output")?);
            fs::write(output, b"fake media").map_err(|error| error.to_string())
        }
    }

    #[test]
    fn execution_uses_tool_seams_and_cleans_interior_temp_files() {
        let directory =
            std::env::temp_dir().join(format!("rkit-video-edit-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let input = directory.join("clip1.mp4");
        let output = directory.join("clip_1.mp4");
        fs::write(&input, b"input").unwrap();
        let args = EditArgs {
            accurate: false,
            crossfade: 0.0,
            start: "1:00".into(),
            end: "5:00".into(),
            input: input.clone(),
        };
        let mut stdout = Vec::new();
        execute(false, args, &FakeTools, &SystemFileAccess, &mut stdout).unwrap();
        assert!(output.exists());
        assert!(String::from_utf8(stdout).unwrap().contains("FILE"));
        let _ = fs::remove_dir_all(directory);
    }
}
