//! Backup-copy behavior for the `bak` utility.

use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const PROGRAM_NAME: &str = "bak";

/// Runs `bak` and writes its process output to the supplied streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if args.len() == 1 && is_flag(&args[0], "-v", "--version") {
        let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
        return 0;
    }
    if args.len() != 1 {
        let _ = writeln!(stdout, "Usage: {PROGRAM_NAME} <file|directory>");
        return 1;
    }

    let source = PathBuf::from(&args[0]);
    if fs::metadata(&source).is_err() {
        let _ = writeln!(stdout, "Usage: {PROGRAM_NAME} <file|directory>");
        return 1;
    }
    let date = match local_calendar_date() {
        Ok(date) => date,
        Err(error) => {
            let _ = writeln!(stderr, "backup failed: determine local date: {error}");
            return 1;
        }
    };
    if let Err(error) = backup_source(&source, &date) {
        let _ = writeln!(stderr, "backup failed: {error}");
        return 1;
    }
    0
}

fn backup_source(source: &Path, date: &str) -> io::Result<PathBuf> {
    let metadata = fs::metadata(source)?;
    let target = available_target(source, date)?;
    if metadata.is_dir() {
        copy_dir(source, &target)?;
    } else {
        copy_file(source, &target, &metadata)?;
    }
    Ok(target)
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}

fn available_target(source: &Path, date: &str) -> io::Result<PathBuf> {
    let mut suffix = String::new();
    loop {
        let mut name = source.as_os_str().to_os_string();
        name.push(format!(".{date}{suffix}"));
        let target = PathBuf::from(name);
        match fs::metadata(&target) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(target),
            Ok(_) | Err(_) => suffix = next_suffix(&suffix),
        }
    }
}

/// Advances the alphabetic collision suffix through the conventional
/// `a`…`z`, `aa`, `ab`, … sequence.
pub fn next_suffix(value: &str) -> String {
    let mut bytes = value.as_bytes().to_vec();
    for index in (0..bytes.len()).rev() {
        if bytes[index] != b'z' {
            bytes[index] += 1;
            return String::from_utf8(bytes).expect("ASCII suffix");
        }
        bytes[index] = b'a';
    }
    bytes.insert(0, b'a');
    String::from_utf8(bytes).expect("ASCII suffix")
}

fn copy_dir(source: &Path, target: &Path) -> io::Result<()> {
    let metadata = fs::metadata(source)?;
    fs::create_dir(target)?;
    set_permissions(target, &metadata)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let child_source = entry.path();
        let child_target = target.join(entry.file_name());
        let link_metadata = fs::symlink_metadata(&child_source)?;
        if link_metadata.file_type().is_dir() {
            copy_dir(&child_source, &child_target)?;
        } else {
            let metadata = fs::metadata(&child_source)?;
            copy_file(&child_source, &child_target, &metadata)?;
        }
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path, metadata: &fs::Metadata) -> io::Result<()> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    set_permissions(target, metadata)?;
    io::copy(&mut input, &mut output)?;
    output.flush()
}

fn set_permissions(path: &Path, metadata: &fs::Metadata) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(metadata.permissions().mode()),
        )?;
    }
    #[cfg(not(unix))]
    {
        fs::set_permissions(path, metadata.permissions())?;
    }
    Ok(())
}

fn local_calendar_date() -> io::Result<String> {
    if let Ok(output) = Command::new("date").arg("+%Y%m%d").output()
        && output.status.success()
    {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Ok(value);
        }
    }
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::other(format!("system clock before Unix epoch: {error}")))?
        .as_secs();
    let (year, month, day) = civil_from_days((seconds / 86_400) as i64);
    Ok(format!("{year:04}{month:02}{day:02}"))
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (year + if month <= 2 { 1 } else { 0 }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn suffixes_increment_and_roll_over() {
        assert_eq!(next_suffix(""), "a");
        assert_eq!(next_suffix("a"), "b");
        assert_eq!(next_suffix("z"), "aa");
        assert_eq!(next_suffix("az"), "ba");
        assert_eq!(next_suffix("zz"), "aaa");
    }

    #[test]
    fn destination_selection_rolls_over_to_double_letters() {
        let root = std::env::temp_dir().join(format!("rkit-bak-rollover-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let source = root.join("source");
        fs::write(&source, b"payload").unwrap();
        fs::write(root.join("source.20260102"), b"taken").unwrap();
        for letter in b'a'..=b'z' {
            fs::write(
                root.join(format!("source.20260102{}", letter as char)),
                b"taken",
            )
            .unwrap();
        }
        let target = backup_source(&source, "20260102").unwrap();
        assert_eq!(target, root.join("source.20260102aa"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn civil_epoch_is_correct() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
    }

    #[cfg(unix)]
    #[test]
    fn copied_file_keeps_mode_and_refuses_existing_destination() {
        let root = std::env::temp_dir().join(format!("rkit-bak-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let source = root.join("source");
        let target = root.join("target");
        fs::write(&source, b"hello").unwrap();
        fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).unwrap();
        copy_file(&source, &target, &fs::metadata(&source).unwrap()).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"hello");
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o640
        );
        assert!(copy_file(&source, &target, &fs::metadata(&source).unwrap()).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fixed_date_seam_selects_deterministic_destination() {
        let root = std::env::temp_dir().join(format!("rkit-bak-date-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let source = root.join("source");
        fs::write(&source, b"fixed").unwrap();
        let target = backup_source(&source, "20260102").unwrap();
        assert_eq!(target, root.join("source.20260102"));
        assert_eq!(fs::read(target).unwrap(), b"fixed");
        fs::remove_dir_all(root).unwrap();
    }
}
