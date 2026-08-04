//! Multi-repository Git status and operation reporting for `repoctl`.

use crate::color::ColorMode;
use std::ffi::OsString;
use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Command as ProcessCommand, ExitStatus, Output, Stdio};
use std::sync::mpsc;
use std::thread;

const PROGRAM_NAME: &str = "repoctl";
const YELLOW: &str = "38;5;226";

/// A command or repository failure with its process exit code.
#[derive(Debug)]
pub struct CliError {
    message: String,
    exit_code: u8,
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 2,
        }
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            exit_code: 1,
        }
    }

    /// Returns the diagnostic intended for standard error.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the process exit code associated with this failure.
    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Help,
    Version,
    Status { repos: Vec<String> },
    Pull { repos: Vec<String> },
    Build { repos: Vec<String> },
    Clone { owner: String, repos: Vec<String> },
}

#[derive(Debug)]
struct RepoResult {
    name: String,
    origin: String,
    status: String,
    details: Vec<String>,
    failed: bool,
}

/// Runs `repoctl` for the provided arguments in the current directory.
pub fn run<I, S, W, E>(
    args: I,
    version: &str,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<u8, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    run_with(args, version, ColorMode::detect_stdout(), stdout, stderr)
}

fn run_with<I, S, W, E>(
    args: I,
    version: &str,
    color: ColorMode,
    stdout: &mut W,
    stderr: &mut E,
) -> Result<u8, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let command = parse_args(args)?;
    match command {
        Command::Help => {
            emit(stdout, stderr, &help(color, version))?;
            Ok(0)
        }
        Command::Version => {
            emit(stdout, stderr, &format!("{PROGRAM_NAME} {version}\n"))?;
            Ok(0)
        }
        Command::Status { repos } => run_local(Operation::Status, repos, color, stdout, stderr),
        Command::Pull { repos } => run_local(Operation::Pull, repos, color, stdout, stderr),
        Command::Build { repos } => run_local(Operation::Build, repos, color, stdout, stderr),
        Command::Clone { owner, repos } => run_clone(&owner, repos, color, stdout, stderr),
    }
}

fn parse_args<I, S>(args: I) -> Result<Command, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let values: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if values.is_empty() {
        return Ok(Command::Help);
    }
    if values
        .iter()
        .any(|value| matches!(value.to_str(), Some("-h" | "-?" | "--help")))
    {
        return Ok(Command::Help);
    }
    if values
        .iter()
        .any(|value| matches!(value.to_str(), Some("-v" | "--version")))
    {
        return Ok(Command::Version);
    }

    let command = values[0].to_str().ok_or_else(|| {
        CliError::usage("parse command: command is not valid UTF-8; use repoctl --help")
    })?;
    let operands = values[1..]
        .iter()
        .map(|value| {
            value.to_str().map(str::to_owned).ok_or_else(|| {
                CliError::usage("parse operand: value is not valid UTF-8; use repoctl --help")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if operands.iter().any(|value| value.starts_with('-')) {
        return Err(CliError::usage(
            "parse operand: unsupported option; use repoctl --help",
        ));
    }

    match command {
        "s" | "status" => Ok(Command::Status { repos: operands }),
        "p" | "pull" => Ok(Command::Pull { repos: operands }),
        "b" | "build" => Ok(Command::Build { repos: operands }),
        "c" | "clone" => {
            let (owner, repos) = operands.split_first().ok_or_else(|| {
                CliError::usage("clone: expected OWNER [REPO ...]; use repoctl --help")
            })?;
            if owner.is_empty() || repos.iter().any(String::is_empty) {
                return Err(CliError::usage(
                    "clone: OWNER and REPO names must not be empty; use repoctl --help",
                ));
            }
            Ok(Command::Clone {
                owner: owner.clone(),
                repos: repos.to_vec(),
            })
        }
        _ => Err(CliError::usage(format!(
            "parse command {command:?}: expected s/status, p/pull, c/clone, or b/build; use repoctl --help"
        ))),
    }
}

#[derive(Clone, Copy)]
enum Operation {
    Status,
    Pull,
    Build,
}

impl Operation {
    fn action(self) -> &'static str {
        match self {
            Self::Status | Self::Pull => "",
            Self::Build => "Building",
        }
    }

    fn streams(self) -> bool {
        matches!(self, Self::Build)
    }
}

fn run_local(
    operation: Operation,
    requested: Vec<String>,
    color: ColorMode,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<u8, CliError> {
    require_command("git", "install Git and ensure git is on PATH")?;
    let mut repos = discover_repositories()?;
    validate_subset(&repos, &requested)?;
    if !requested.is_empty() {
        repos.retain(|repo| requested.iter().any(|name| name == &repo.name));
    }
    for repo in &mut repos {
        repo.origin = origin(&repo.name);
    }
    repos.sort_by(|left, right| {
        left.origin
            .cmp(&right.origin)
            .then(left.name.cmp(&right.name))
    });
    let widths = column_widths(&repos);

    let mut failed = false;
    for repo in repos {
        if operation.streams() {
            emit(
                stdout,
                stderr,
                &render_processing(&repo, operation.action(), color, widths),
            )?;
        }
        let result = match operation {
            Operation::Status => Ok(status_repo(repo)),
            Operation::Pull => Ok(pull_repo(repo)),
            Operation::Build => build_repo(repo, color, stdout, stderr),
        }?;
        failed |= result.failed;
        if operation.streams() {
            emit_pending_details(stdout, stderr, &result.details)?;
            emit(stdout, stderr, &render_final_status(&result, color))?;
        } else {
            emit(stdout, stderr, &render_result(&result, color, widths))?;
        }
    }
    Ok(if failed { 1 } else { 0 })
}

fn run_clone(
    owner: &str,
    requested: Vec<String>,
    color: ColorMode,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<u8, CliError> {
    require_command("git", "install Git and ensure git is on PATH")?;
    let names = if requested.is_empty() {
        require_command("gh", "install GitHub CLI and authenticate with gh")?;
        list_remote_repositories(owner)?
    } else {
        requested
    };
    if names.iter().any(|name| !valid_repo_name(name)) {
        return Err(CliError::usage(
            "clone: repository names must be simple directory names; use repoctl --help",
        ));
    }

    let mut repos = names
        .into_iter()
        .map(|name| RepoResult {
            origin: format!("https://github.com/{owner}/{name}.git"),
            name,
            status: String::new(),
            details: Vec::new(),
            failed: false,
        })
        .collect::<Vec<_>>();
    repos.sort_by(|left, right| {
        left.origin
            .cmp(&right.origin)
            .then(left.name.cmp(&right.name))
    });
    let widths = column_widths(&repos);
    let mut failed = false;
    for repo in repos {
        emit(
            stdout,
            stderr,
            &render_processing(&repo, "Cloning", color, widths),
        )?;
        let result = clone_repo(owner, repo, stdout, stderr)?;
        failed |= result.failed;
        emit_pending_details(stdout, stderr, &result.details)?;
        emit(stdout, stderr, &render_final_status(&result, color))?;
    }
    Ok(if failed { 1 } else { 0 })
}

fn discover_repositories() -> Result<Vec<RepoResult>, CliError> {
    let entries = fs::read_dir(".").map_err(|error| {
        CliError::runtime(format!(
            "read repository directory: {error}; verify the current directory is readable and retry"
        ))
    })?;
    let mut repos = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            CliError::runtime(format!(
                "read repository entry: {error}; verify the current directory is readable and retry"
            ))
        })?;
        let name = entry.file_name().into_string().map_err(|_| {
            CliError::runtime(
                "discover repositories: an entry name is not valid UTF-8; rename it and retry",
            )
        })?;
        if name.starts_with('.') || !entry.file_type().map_err(|error| {
            CliError::runtime(format!(
                "inspect repository entry {name:?}: {error}; verify directory permissions and retry"
            ))
        })?.is_dir() {
            continue;
        }
        let git_dir = Path::new(&name).join(".git");
        if git_dir.is_dir() {
            repos.push(RepoResult {
                name,
                origin: String::new(),
                status: String::new(),
                details: Vec::new(),
                failed: false,
            });
        }
    }
    repos.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(repos)
}

fn validate_subset(repos: &[RepoResult], requested: &[String]) -> Result<(), CliError> {
    for name in requested {
        if !repos.iter().any(|repo| repo.name == *name) {
            return Err(CliError::usage(format!(
                "select repository {name:?}: no matching immediate Git repository; use repoctl --help"
            )));
        }
    }
    Ok(())
}

fn status_repo(mut repo: RepoResult) -> RepoResult {
    let branch = command_in_repo(&repo.name, &["branch", "--show-current"]);
    let status = command_in_repo(&repo.name, &["status", "--porcelain"]);
    match (branch, status) {
        (Ok(branch), Ok(status)) => {
            let branch = if branch.trim().is_empty() {
                "(detached)"
            } else {
                branch.trim()
            };
            repo.status = format!(
                "{} {branch}",
                if status.trim().is_empty() {
                    "👍"
                } else {
                    "❌"
                }
            );
        }
        (branch, status) => {
            repo.status = "Status failed".to_owned();
            repo.failed = true;
            append_command_details(&mut repo.details, branch.err());
            append_command_details(&mut repo.details, status.err());
        }
    }
    repo
}

fn pull_repo(mut repo: RepoResult) -> RepoResult {
    let remote = command_in_repo(&repo.name, &["ls-remote"]);
    if let Err(error) = remote {
        repo.status = "Remote unavailable".to_owned();
        repo.failed = true;
        repo.details.push(error);
        return repo;
    }
    match ProcessCommand::new("git")
        .arg("pull")
        .current_dir(&repo.name)
        .output()
    {
        Ok(output) if output.status.success() => {
            let text = combined_output(&output);
            repo.status = if routine_pull_output(&text) {
                "Already up to date"
            } else {
                "Pulled"
            }
            .to_owned();
            if repo.status == "Pulled" {
                append_text_details(&mut repo.details, &text);
            }
        }
        Ok(output) => {
            repo.status = "Pull failed".to_owned();
            repo.failed = true;
            append_text_details(&mut repo.details, &combined_output(&output));
            repo.details.push(format!(
                "git pull exited with {}; verify the repository and remote before retrying",
                output.status
            ));
        }
        Err(error) => {
            repo.status = "Pull failed".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "run git pull: {error}; verify Git and repository permissions before retrying"
            ));
        }
    }
    repo
}

fn build_repo(
    mut repo: RepoResult,
    color: ColorMode,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<RepoResult, CliError> {
    let path = Path::new(&repo.name).join("build.sh");
    match fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() && executable(&metadata) => {}
        Ok(_) => {
            repo.status = "No build.sh".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "{} is missing or not executable; restore an executable build.sh and retry",
                path.display()
            ));
            return Ok(repo);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            repo.status = "No build.sh".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "{} is missing; add an executable build.sh and retry",
                path.display()
            ));
            return Ok(repo);
        }
        Err(error) => {
            repo.status = "Build failed".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "inspect {}: {error}; verify permissions and retry",
                path.display()
            ));
            return Ok(repo);
        }
    }
    let mut command = ProcessCommand::new("./build.sh");
    command.current_dir(&repo.name);
    configure_build_color(
        &mut command,
        color,
        std::env::var_os("GOVERNA_FORCE_TTY").as_deref(),
    );
    match stream_command(&mut command, stdout, stderr)? {
        Ok(output) if output.status.success() => {
            repo.status = "Built".to_owned();
        }
        Ok(output) => {
            repo.status = "Build failed".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "./build.sh exited with {}; verify the repository build and retry",
                output.status
            ));
        }
        Err(error) => {
            repo.status = "Build failed".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "run ./build.sh: {error}; verify the script is executable and retry"
            ));
        }
    }
    Ok(repo)
}

fn clone_repo(
    _owner: &str,
    mut repo: RepoResult,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<RepoResult, CliError> {
    match fs::metadata(&repo.name) {
        Ok(_) => {
            repo.status = "Skipped".to_owned();
            return Ok(repo);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            repo.status = "Clone failed".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "inspect clone destination {:?}: {error}; verify permissions and retry",
                repo.name
            ));
            return Ok(repo);
        }
    }
    let mut command = ProcessCommand::new("git");
    command.args(["clone", &repo.origin, &repo.name]);
    match stream_command(&mut command, stdout, stderr)? {
        Ok(output) if output.status.success() => {
            repo.status = "Cloned".to_owned();
        }
        Ok(output) => {
            repo.status = "Clone failed".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "git clone exited with {}; verify the owner, repository, and credentials and retry",
                output.status
            ));
        }
        Err(error) => {
            repo.status = "Clone failed".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "run git clone: {error}; install Git and verify PATH before retrying"
            ));
        }
    }
    Ok(repo)
}

fn origin(repo: &str) -> String {
    match command_in_repo_output(repo, &["remote", "get-url", "origin"]) {
        Ok(output) => {
            let origin = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if origin.is_empty() {
                "<no origin>".to_owned()
            } else {
                origin
            }
        }
        Err(_) => "<no origin>".to_owned(),
    }
}

fn command_in_repo(repo: &str, args: &[&str]) -> Result<String, String> {
    let output = command_in_repo_output(repo, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn command_in_repo_output(repo: &str, args: &[&str]) -> Result<Output, String> {
    ProcessCommand::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|error| {
            format!(
                "run git {} in {repo}: {error}; verify Git and repository permissions",
                args.join(" ")
            )
        })
        .and_then(|output| {
            if output.status.success() {
                Ok(output)
            } else {
                let detail = combined_output(&output);
                Err(format!(
                    "git {} in {repo} exited with {}{}",
                    args.join(" "),
                    output.status,
                    if detail.is_empty() {
                        String::new()
                    } else {
                        format!(": {detail}")
                    }
                ))
            }
        })
}

fn list_remote_repositories(owner: &str) -> Result<Vec<String>, CliError> {
    let output = ProcessCommand::new("gh")
        .args(["repo", "list", owner, "--json", "name", "--jq", ".[].name"])
        .output()
        .map_err(|error| {
            CliError::runtime(format!(
                "list repositories for {owner:?}: {error}; authenticate with gh and retry"
            ))
        })?;
    if !output.status.success() {
        return Err(CliError::runtime(format!(
            "list repositories for {owner:?}: gh exited with {}; authenticate with gh and retry{}",
            output.status,
            detail_suffix(&combined_output(&output)),
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect())
}

fn require_command(name: &str, guidance: &str) -> Result<(), CliError> {
    match ProcessCommand::new(name).arg("--version").output() {
        Ok(output) if output.status.success() => Ok(()),
        Ok(output) => Err(CliError::runtime(format!(
            "check {name}: command exited with {}; {guidance} and retry",
            output.status
        ))),
        Err(error) => Err(CliError::runtime(format!(
            "check {name}: {error}; {guidance} and retry"
        ))),
    }
}

fn valid_repo_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains('/') && !name.contains('\\')
}

#[cfg(unix)]
fn executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_metadata: &fs::Metadata) -> bool {
    true
}

fn append_command_details(details: &mut Vec<String>, error: Option<String>) {
    if let Some(error) = error {
        details.push(error);
    }
}

fn append_text_details(details: &mut Vec<String>, text: &str) {
    details.extend(text.lines().map(str::to_owned));
}

#[derive(Debug)]
struct StreamOutput {
    status: ExitStatus,
}

enum StreamMessage {
    Line(Vec<u8>),
    ReadError(String),
}

fn stream_command(
    command: &mut ProcessCommand,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> Result<Result<StreamOutput, String>, CliError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return Ok(Err(format!(
                "start child process: {error}; verify the command and retry"
            )));
        }
    };
    let child_stdout = child.stdout.take().expect("piped child stdout");
    let child_stderr = child.stderr.take().expect("piped child stderr");
    let (sender, receiver) = mpsc::channel();
    let stdout_reader = spawn_stream_reader(child_stdout, sender.clone(), "stdout");
    let stderr_reader = spawn_stream_reader(child_stderr, sender, "stderr");
    for message in receiver {
        let bytes = match message {
            StreamMessage::Line(bytes) => bytes,
            StreamMessage::ReadError(error) => {
                cleanup_child(&mut child, stdout_reader, stderr_reader);
                return Ok(Err(error));
            }
        };
        let line = String::from_utf8_lossy(&bytes)
            .trim_end_matches(['\r', '\n'])
            .to_owned();
        if let Err(error) = emit_detail(stdout, stderr, &line) {
            cleanup_child(&mut child, stdout_reader, stderr_reader);
            return Err(error);
        }
    }

    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
    let status = child.wait().map_err(|error| {
        CliError::runtime(format!(
            "wait for child process: {error}; verify process permissions and retry"
        ))
    })?;
    Ok(Ok(StreamOutput { status }))
}

fn spawn_stream_reader<R: Read + Send + 'static>(
    stream: R,
    sender: mpsc::Sender<StreamMessage>,
    source: &'static str,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        loop {
            let mut bytes = Vec::new();
            match reader.read_until(b'\n', &mut bytes) {
                Ok(0) => return,
                Ok(_) => {
                    if sender.send(StreamMessage::Line(bytes)).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = sender.send(StreamMessage::ReadError(format!(
                        "read child {source}: {error}; retry the repository operation"
                    )));
                    return;
                }
            }
        }
    })
}

fn cleanup_child(
    child: &mut std::process::Child,
    stdout_reader: thread::JoinHandle<()>,
    stderr_reader: thread::JoinHandle<()>,
) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = stdout_reader.join();
    let _ = stderr_reader.join();
}

fn routine_pull_output(text: &str) -> bool {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    matches!(
        (lines.next(), lines.next()),
        (
            Some(
                "Already up to date"
                    | "Already up to date."
                    | "Already up-to-date"
                    | "Already up-to-date."
            ),
            None
        )
    )
}

fn configure_build_color(
    command: &mut ProcessCommand,
    color: ColorMode,
    inherited_force_tty: Option<&std::ffi::OsStr>,
) {
    if color.enabled() && inherited_force_tty.is_none() {
        command.env("GOVERNA_FORCE_TTY", "1");
    }
}

fn combined_output(output: &Output) -> String {
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&stderr);
    }
    text.trim().to_owned()
}

fn detail_suffix(text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        format!(": {text}")
    }
}

#[derive(Clone, Copy)]
struct ColumnWidths {
    repo: usize,
    origin: usize,
}

/// Strips a trailing literal `.git` suffix for display — noise most of the
/// time and never load-bearing for a printed Origin column. The `origin`
/// field itself is left untouched everywhere it's used functionally (the
/// sort comparators and, for `run_clone`, the actual `git clone` argument),
/// since only display reads through this helper.
fn display_origin(origin: &str) -> &str {
    origin.strip_suffix(".git").unwrap_or(origin)
}

fn column_widths(results: &[RepoResult]) -> ColumnWidths {
    ColumnWidths {
        repo: results
            .iter()
            .map(|result| result.name.chars().count())
            .max()
            .unwrap_or(0),
        origin: results
            .iter()
            .map(|result| display_origin(&result.origin).chars().count())
            .max()
            .unwrap_or(0),
    }
}

fn render_processing(
    result: &RepoResult,
    action: &str,
    color: ColorMode,
    widths: ColumnWidths,
) -> String {
    let origin = display_origin(&result.origin);
    let row = format!(
        "==> {}{}{}{}",
        result.name,
        " ".repeat(widths.repo - result.name.chars().count() + 4),
        origin,
        " ".repeat(widths.origin - origin.chars().count() + 4),
    );
    format!("{}\n", color.paint(YELLOW, &(row + action)))
}

fn render_result(result: &RepoResult, color: ColorMode, widths: ColumnWidths) -> String {
    let origin = display_origin(&result.origin);
    let row = format!(
        "==> {}{}{}{}{}",
        result.name,
        " ".repeat(widths.repo - result.name.chars().count() + 4),
        origin,
        " ".repeat(widths.origin - origin.chars().count() + 4),
        result.status,
    );
    let mut output = format!("{}\n", color.paint(YELLOW, &row));
    for detail in &result.details {
        output.push_str("    ");
        output.push_str(detail);
        output.push('\n');
    }
    output
}

fn render_final_status(result: &RepoResult, color: ColorMode) -> String {
    format!(
        "{}\n",
        color.paint(YELLOW, &format!("    {}", result.status))
    )
}

fn emit_pending_details(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    details: &[String],
) -> Result<(), CliError> {
    for detail in details {
        emit_detail(stdout, stderr, detail)?;
    }
    Ok(())
}

fn emit_detail(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    detail: &str,
) -> Result<(), CliError> {
    emit(stdout, stderr, &format!("    {detail}\n"))
}

fn emit(stdout: &mut impl Write, _stderr: &mut impl Write, text: &str) -> Result<(), CliError> {
    if let Err(error) = stdout
        .write_all(text.as_bytes())
        .and_then(|_| stdout.flush())
    {
        return Err(CliError::runtime(format!(
            "write repoctl output: {error}; verify standard output is writable and retry"
        )));
    }
    Ok(())
}

fn help(color: ColorMode, version: &str) -> String {
    let name = color.paint("38;5;15", PROGRAM_NAME);
    format!(
        "{name} v{version}\n\
Control a collection of local Git repositories.\n\n\
Usage\n  {name} COMMAND [REPO ...]\n  {name} clone OWNER [REPO ...]\n\n\
Commands\n  s, status  Show repository status\n  p, pull    Pull selected repositories\n  c, clone   Clone an owner's repositories\n  b, build   Run ./build.sh in selected repositories\n\n\
Options\n  -v, --version  Print version and exit\n  -h, -?, --help Show this help message and exit\n\n\
Summary rows are sorted by Origin and show Repo, Origin, and operation Status.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BrokenWriter;

    impl Write for BrokenWriter {
        fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed fixture"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn display_origin_strips_trailing_dot_git_only() {
        assert_eq!(
            display_origin("https://github.com/kquo/bits.git"),
            "https://github.com/kquo/bits"
        );
        assert_eq!(
            display_origin("git@github.com:kquo/bits.git"),
            "git@github.com:kquo/bits"
        );
        assert_eq!(
            display_origin("https://github.com/kquo/bits"),
            "https://github.com/kquo/bits"
        );
        assert_eq!(display_origin("<no origin>"), "<no origin>");
        assert_eq!(display_origin(""), "");
    }

    #[test]
    fn column_widths_and_rendering_measure_the_trimmed_origin() {
        let results = vec![RepoResult {
            name: "bits".to_owned(),
            origin: "https://github.com/kquo/bits.git".to_owned(),
            status: "👍 main".to_owned(),
            details: Vec::new(),
            failed: false,
        }];
        let widths = column_widths(&results);
        // "https://github.com/kquo/bits" (trimmed) is 28 chars; the raw
        // (untrimmed, .git-suffixed) string would be 32.
        assert_eq!(widths.origin, 28);
        let output = render_result(&results[0], ColorMode::new(false), widths);
        assert!(output.contains("https://github.com/kquo/bits "));
        assert!(!output.contains(".git"));
    }

    #[test]
    fn terminal_summary_paints_all_three_values_yellow() {
        let results = vec![RepoResult {
            name: "bits".to_owned(),
            origin: "https://github.com/kquo/bits.git".to_owned(),
            status: "👍 main".to_owned(),
            details: vec!["diagnostic".to_owned()],
            failed: false,
        }];
        let output = render_result(&results[0], ColorMode::new(true), column_widths(&results));
        assert!(output.starts_with("\x1b[38;5;226m==> bits"));
        assert!(output.contains("👍 main\x1b[0m\n"));
        assert!(output.ends_with("    diagnostic\n"));
        assert_eq!(
            render_final_status(&results[0], ColorMode::new(true)),
            "\x1b[38;5;226m    👍 main\x1b[0m\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn stream_command_forwards_final_partial_line() {
        let mut command = ProcessCommand::new("/bin/sh");
        command.args(["-c", "printf partial"]);
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        let output = stream_command(&mut command, &mut stdout, &mut stderr)
            .expect("write output")
            .expect("spawn child");

        assert!(output.status.success());
        assert_eq!(String::from_utf8(stdout).unwrap(), "    partial\n");
        assert!(stderr.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn stream_command_cleans_up_child_after_output_failure() {
        let mut command = ProcessCommand::new("/bin/sh");
        command.args(["-c", "printf 'detail\\n'; exec sleep 30"]);
        let mut stdout = BrokenWriter;
        let mut stderr = Vec::new();

        let error = stream_command(&mut command, &mut stdout, &mut stderr)
            .expect_err("broken output must fail");

        assert_eq!(error.exit_code(), 1);
        assert_eq!(
            error.message(),
            "write repoctl output: closed fixture; verify standard output is writable and retry"
        );
        assert!(stderr.is_empty());
    }

    #[test]
    fn build_color_injection_respects_resolved_color_and_inherited_values() {
        for (color, inherited, expected) in [
            (true, None, Some("1")),
            (false, None, None),
            (true, Some("0"), None),
            (true, Some("1"), None),
            (true, Some(""), None),
        ] {
            let mut command = ProcessCommand::new("./build.sh");
            configure_build_color(
                &mut command,
                ColorMode::new(color),
                inherited.map(std::ffi::OsStr::new),
            );
            let configured = command
                .get_envs()
                .find(|(name, _)| *name == "GOVERNA_FORCE_TTY")
                .and_then(|(_, value)| value)
                .and_then(std::ffi::OsStr::to_str);
            assert_eq!(configured, expected);
        }
    }
}
