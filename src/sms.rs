//! SMS-sending and config-file migration behavior for the `sms` utility.

use crate::pman::{HttpRequest, HttpTransport, TcpHttpTransport};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const PROGRAM_NAME: &str = "sms";

/// Runs `sms` and writes its process output to the supplied streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    run_with(args, version, &TcpHttpTransport, stdout, stderr)
}

/// Runs `sms` against an injectable [`HttpTransport`], independent of the
/// network.
pub fn run_with<I, S, H, W, E>(
    args: I,
    version: &str,
    transport: &H,
    stdout: &mut W,
    stderr: &mut E,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    H: HttpTransport,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();

    // Go's `printUsage` always exits 0, including for a bad argument count.
    if args.is_empty() || args.len() > 2 {
        let _ = write!(stdout, "{}", usage_text(version));
        return 0;
    }

    if args.len() == 1 {
        if is_flag(&args[0], "-v", "--version") {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            return 0;
        }
        if is_help_flag(&args[0]) {
            let _ = write!(stdout, "{}", usage_text(version));
            return 0;
        }
        if args[0] == "-y" || args[0] == "--init" {
            return create_skeleton_config_file(stdout, stderr);
        }
        let _ = write!(stdout, "{}", usage_text(version));
        return 0;
    }

    // Go silently treats a `-v` combined with another operand as the phone
    // number and attempts to send; reject it instead, matching the `pman`
    // precedent for a version flag combined with other operands.
    if args.iter().any(|arg| is_flag(arg, "-v", "--version")) {
        let _ = writeln!(
            stderr,
            "{PROGRAM_NAME}: version flag cannot be combined with other operands"
        );
        return 2;
    }

    let (svcurl, svckey) = match process_config_file(stderr) {
        Ok(pair) => pair,
        Err(message) => {
            // Go prints config-resolution errors to stdout, not stderr.
            let _ = writeln!(stdout, "{message}");
            return 1;
        }
    };

    let tel = args[0].to_string_lossy().into_owned();
    let msg = args[1].to_string_lossy().into_owned();
    let _ = writeln!(stdout, "{svckey}  {tel}  {msg}");

    let body = form_urlencode(&[("key", &svckey), ("message", &msg), ("phone", &tel)]);
    let request = HttpRequest {
        method: "POST".to_owned(),
        url: svcurl,
        headers: vec![(
            "Content-Type".to_owned(),
            "application/x-www-form-urlencoded".to_owned(),
        )],
        body: Some(body.into_bytes()),
    };

    match transport.send(&request) {
        Ok(response) if response.status == 200 => 0,
        Ok(response) => {
            let _ = writeln!(stdout, "Error. HTTP error code = {}", response.status);
            1
        }
        Err(error) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: request failed: {error}");
            1
        }
    }
}

fn usage_text(version: &str) -> String {
    format!(
        "SMS CLI utility {version}\n\
{PROGRAM_NAME} <CellPhoneNum> <Message>\n\
{PROGRAM_NAME} -v | --version\n\
{PROGRAM_NAME} -y Create skeleton ~/.config/{PROGRAM_NAME}/config.ini file\n\
Visit https://textbelt.com for more info.\n"
    )
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}

fn is_help_flag(value: &OsString) -> bool {
    value == "-?" || value == "-h" || value == "--help"
}

fn create_skeleton_config_file<W: Write, E: Write>(stdout: &mut W, stderr: &mut E) -> u8 {
    let cfgfile = match config_path(stderr) {
        Ok(path) => path,
        Err(error) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
            return 1;
        }
    };
    if cfgfile.exists() {
        let _ = writeln!(stdout, "There's already a '{}' file.", cfgfile.display());
        return 0;
    }
    if let Err(error) = write_skeleton_config(&cfgfile) {
        let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
        return 1;
    }
    0
}

fn write_skeleton_config(cfgfile: &Path) -> io::Result<()> {
    if let Some(parent) = cfgfile.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = "# Edit below values accordingly\n\
[global]\n\
svcurl = https://textbelt.com/text\n\
svckey = textbelt\n";
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(cfgfile)?
            .write_all(content.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        fs::write(cfgfile, content)?;
    }
    Ok(())
}

/// Resolves the config path a `sms` process (not a test) reads: the XDG
/// path, migrating the legacy `~/.smsrc` into it on first use.
fn config_path<E: Write>(stderr: &mut E) -> io::Result<PathBuf> {
    let cfg_dir = xdg_config_dir()?;
    let new_path = cfg_dir.join(PROGRAM_NAME).join("config.ini");
    let home = home_dir()?;
    let old_path = home.join(format!(".{PROGRAM_NAME}rc"));
    if let Err(error) = migrate_if_needed(&old_path, &new_path, 0o600, stderr) {
        let _ = writeln!(stderr, "{PROGRAM_NAME}: migration warning: {error}");
    }
    Ok(new_path)
}

fn xdg_config_dir() -> io::Result<PathBuf> {
    if let Some(value) = env::var_os("XDG_CONFIG_HOME")
        && !value.is_empty()
    {
        let path = PathBuf::from(&value);
        if path.is_absolute() {
            return Ok(path);
        }
    }
    Ok(home_dir()?.join(".config"))
}

fn home_dir() -> io::Result<PathBuf> {
    env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::other("HOME is not set"))
}

/// Moves `old_path` to `new_path` when eligible, mirroring the Go original's
/// one-shot legacy migration: skip a symlinked `old_path` with a warning,
/// warn and prefer `new_path` when both exist, and fall back to copy+delete
/// across a filesystem boundary. Sets `mode` on `new_path` in every
/// migration outcome.
fn migrate_if_needed<E: Write>(
    old_path: &Path,
    new_path: &Path,
    mode: u32,
    stderr: &mut E,
) -> io::Result<()> {
    let old_metadata = match fs::symlink_metadata(old_path) {
        Ok(metadata) => metadata,
        Err(_) => return Ok(()), // nothing to migrate
    };

    if old_metadata.file_type().is_symlink() {
        let _ = writeln!(
            stderr,
            "{PROGRAM_NAME}: {} is a symlink; skipping auto-migration. Move it to {} manually.",
            old_path.display(),
            new_path.display()
        );
        return Ok(());
    }

    if new_path.exists() {
        let _ = writeln!(
            stderr,
            "{PROGRAM_NAME}: both {} and {} exist; using {}. Delete the old file when ready.",
            old_path.display(),
            new_path.display(),
            new_path.display()
        );
        return Ok(());
    }

    if let Some(parent) = new_path.parent() {
        fs::create_dir_all(parent)?;
    }

    match fs::rename(old_path, new_path) {
        Ok(()) => {}
        Err(error) if is_cross_device(&error) => {
            fs::copy(old_path, new_path)?;
            fs::remove_file(old_path)?;
        }
        Err(error) => return Err(error),
    }

    set_mode(new_path, mode)?;

    let _ = writeln!(
        stderr,
        "{PROGRAM_NAME}: migrated {} -> {}",
        old_path.display(),
        new_path.display()
    );
    Ok(())
}

fn is_cross_device(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::CrossesDevices
}

fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
    Ok(())
}

/// Reads and validates `cfgfile`'s `[global]` `svcurl`/`svckey` values,
/// mirroring Go's silent degrade-to-empty on an unreadable or malformed
/// file (the underlying `ini.LoadFile` error is discarded there too).
fn load_config(cfgfile: &Path) -> Result<(String, String), String> {
    if !cfgfile.exists() {
        return Err(format!(
            "Error. Missing '{}' file. Run '{PROGRAM_NAME} -y' to create a new one.",
            cfgfile.display()
        ));
    }
    let contents = fs::read_to_string(cfgfile).unwrap_or_default();
    let svcurl = parse_ini_value(&contents, "global", "svcurl").unwrap_or_default();
    if svcurl.is_empty() {
        return Err(format!(
            "Error. svcurl not defined in '{}' file.",
            cfgfile.display()
        ));
    }
    let svckey = parse_ini_value(&contents, "global", "svckey").unwrap_or_default();
    if svckey.is_empty() {
        return Err(format!(
            "Error. svckey not defined in '{}' file.",
            cfgfile.display()
        ));
    }
    Ok((svcurl, svckey))
}

fn process_config_file<E: Write>(stderr: &mut E) -> Result<(String, String), String> {
    let cfgfile = config_path(stderr)
        .map_err(|error| format!("Error. Cannot resolve config path: {error}"))?;
    load_config(&cfgfile)
}

/// Parses a trivial `[section]` / `key = value` INI format: `#`/`;` line
/// comments, no quoting, no multi-line values.
fn parse_ini_value(contents: &str, section: &str, key: &str) -> Option<String> {
    let mut current_section = String::new();
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            current_section = name.trim().to_owned();
            continue;
        }
        if current_section != section {
            continue;
        }
        if let Some((found_key, value)) = line.split_once('=')
            && found_key.trim() == key
        {
            return Some(value.trim().to_owned());
        }
    }
    None
}

fn form_urlencode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| format!("{}={}", percent_encode(key), percent_encode(value)))
        .collect::<Vec<_>>()
        .join("&")
}

fn percent_encode(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char);
            }
            b' ' => output.push('+'),
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pman::HttpResponse;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rkit-sms-unit-{}-{number}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }

    struct FakeTransport {
        response: Mutex<Option<Result<HttpResponse, String>>>,
        captured: Mutex<Option<HttpRequest>>,
    }

    impl HttpTransport for FakeTransport {
        fn send(&self, request: &HttpRequest) -> io::Result<HttpResponse> {
            *self.captured.lock().unwrap() = Some(request.clone());
            match self.response.lock().unwrap().take().unwrap() {
                Ok(response) => Ok(response),
                Err(message) => Err(io::Error::other(message)),
            }
        }
    }

    fn ok_transport(status: u16, body: &[u8]) -> FakeTransport {
        FakeTransport {
            response: Mutex::new(Some(Ok(HttpResponse {
                status,
                body: body.to_vec(),
            }))),
            captured: Mutex::new(None),
        }
    }

    #[test]
    fn percent_encode_escapes_reserved_bytes_and_spaces() {
        assert_eq!(percent_encode("Hello world!"), "Hello+world%21");
        assert_eq!(percent_encode("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn form_urlencode_joins_pairs_in_call_order() {
        let body = form_urlencode(&[
            ("key", "textbelt"),
            ("message", "hi there"),
            ("phone", "555"),
        ]);
        assert_eq!(body, "key=textbelt&message=hi+there&phone=555");
    }

    #[test]
    fn parse_ini_value_reads_the_named_section_and_ignores_comments() {
        let contents = "# comment\n[global]\nsvcurl = https://textbelt.com/text\nsvckey=abc\n\n[other]\nsvckey=wrong\n";
        assert_eq!(
            parse_ini_value(contents, "global", "svcurl").as_deref(),
            Some("https://textbelt.com/text")
        );
        assert_eq!(
            parse_ini_value(contents, "global", "svckey").as_deref(),
            Some("abc")
        );
        assert_eq!(parse_ini_value(contents, "global", "missing"), None);
    }

    #[test]
    fn load_config_reports_missing_file_and_missing_keys() {
        let directory = temp_dir();
        let missing = directory.join("config.ini");
        let error = load_config(&missing).unwrap_err();
        assert!(error.contains("Missing"));

        fs::write(&missing, "[global]\nsvckey = abc\n").unwrap();
        let error = load_config(&missing).unwrap_err();
        assert!(error.contains("svcurl not defined"));

        fs::write(&missing, "[global]\nsvcurl = https://x\n").unwrap();
        let error = load_config(&missing).unwrap_err();
        assert!(error.contains("svckey not defined"));

        fs::write(&missing, "[global]\nsvcurl = https://x\nsvckey = abc\n").unwrap();
        let (svcurl, svckey) = load_config(&missing).unwrap();
        assert_eq!(svcurl, "https://x");
        assert_eq!(svckey, "abc");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn migrate_skips_when_nothing_to_migrate() {
        let directory = temp_dir();
        let old_path = directory.join(".smsrc");
        let new_path = directory.join("config.ini");
        let mut stderr = Vec::new();
        migrate_if_needed(&old_path, &new_path, 0o600, &mut stderr).unwrap();
        assert!(stderr.is_empty());
        assert!(!new_path.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn migrate_moves_legacy_file_and_sets_mode() {
        let directory = temp_dir();
        let old_path = directory.join(".smsrc");
        let new_path = directory.join("nested").join("config.ini");
        fs::write(&old_path, "[global]\nsvckey = secret\n").unwrap();
        let mut stderr = Vec::new();
        migrate_if_needed(&old_path, &new_path, 0o600, &mut stderr).unwrap();

        assert!(!old_path.exists());
        assert_eq!(
            fs::read_to_string(&new_path).unwrap(),
            "[global]\nsvckey = secret\n"
        );
        let stderr_text = String::from_utf8(stderr).unwrap();
        assert!(stderr_text.contains("migrated"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&new_path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn migrate_skips_symlink_with_warning() {
        let directory = temp_dir();
        let target = directory.join("actual-config");
        fs::write(&target, "REAL").unwrap();
        let old_path = directory.join(".smsrc");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &old_path).unwrap();
        let new_path = directory.join("config.ini");

        let mut stderr = Vec::new();
        migrate_if_needed(&old_path, &new_path, 0o600, &mut stderr).unwrap();
        assert!(old_path.is_symlink());
        assert!(!new_path.exists());
        assert!(String::from_utf8(stderr).unwrap().contains("symlink"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn migrate_warns_and_prefers_new_path_when_both_exist() {
        let directory = temp_dir();
        let old_path = directory.join(".smsrc");
        let new_path = directory.join("config.ini");
        fs::write(&old_path, "OLD").unwrap();
        fs::write(&new_path, "NEW").unwrap();

        let mut stderr = Vec::new();
        migrate_if_needed(&old_path, &new_path, 0o600, &mut stderr).unwrap();
        assert_eq!(fs::read_to_string(&new_path).unwrap(), "NEW");
        assert!(old_path.exists());
        assert!(String::from_utf8(stderr).unwrap().contains("both"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn bad_argument_count_prints_usage_and_exits_zero() {
        let transport = ok_transport(200, b"");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            Vec::<&str>::new(),
            "1.0.0",
            &transport,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert!(
            String::from_utf8(stdout)
                .unwrap()
                .contains("SMS CLI utility")
        );
        assert!(stderr.is_empty());
    }

    #[test]
    fn version_flag_alone_prints_exact_line() {
        let transport = ok_transport(200, b"");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(["-v"], "1.0.0", &transport, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, b"sms v1.0.0\n");
    }

    #[test]
    fn version_flag_combined_with_operand_is_rejected() {
        let transport = ok_transport(200, b"");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            ["-v", "5551234567"],
            "1.0.0",
            &transport,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 2);
        assert!(stdout.is_empty());
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("version flag cannot be combined with other operands")
        );
    }

    #[test]
    fn unrecognized_single_argument_prints_usage_and_exits_zero() {
        let transport = ok_transport(200, b"");
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(["-x"], "1.0.0", &transport, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert!(
            String::from_utf8(stdout)
                .unwrap()
                .contains("SMS CLI utility")
        );
    }
}
