//! Multi-repository Git status and operation reporting for `repoctl`.

use crate::color::ColorMode;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command as ProcessCommand, Output};

const PROGRAM_NAME: &str = "repoctl";
pub const PROGRAM_VERSION: &str = "0.1.0";
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
pub fn run<I, S, W, E>(args: I, stdout: &mut W, stderr: &mut E) -> Result<u8, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    run_with(args, ColorMode::detect_stdout(), stdout, stderr)
}

fn run_with<I, S, W, E>(
    args: I,
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
            emit(stdout, stderr, &help(color))?;
            Ok(0)
        }
        Command::Version => {
            emit(
                stdout,
                stderr,
                &format!("{PROGRAM_NAME} {PROGRAM_VERSION}\n"),
            )?;
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
        let result = match operation {
            Operation::Status => status_repo(repo),
            Operation::Pull => pull_repo(repo),
            Operation::Build => build_repo(repo),
        };
        failed |= result.failed;
        emit(stdout, stderr, &render_result(&result, color, widths))?;
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
        let result = clone_repo(owner, repo);
        failed |= result.failed;
        emit(stdout, stderr, &render_result(&result, color, widths))?;
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
    match command_in_repo_output(&repo.name, &["pull"]) {
        Ok(output) => {
            let text = combined_output(&output);
            repo.status =
                if text.contains("Already up to date") || text.contains("Already up-to-date") {
                    "Already up to date"
                } else {
                    "Pulled"
                }
                .to_owned();
            append_text_details(&mut repo.details, &text);
        }
        Err(error) => {
            repo.status = "Pull failed".to_owned();
            repo.failed = true;
            repo.details.push(error);
        }
    }
    repo
}

fn build_repo(mut repo: RepoResult) -> RepoResult {
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
            return repo;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            repo.status = "No build.sh".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "{} is missing; add an executable build.sh and retry",
                path.display()
            ));
            return repo;
        }
        Err(error) => {
            repo.status = "Build failed".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "inspect {}: {error}; verify permissions and retry",
                path.display()
            ));
            return repo;
        }
    }
    match ProcessCommand::new("./build.sh")
        .current_dir(&repo.name)
        .output()
    {
        Ok(output) if output.status.success() => {
            repo.status = "Built".to_owned();
            append_text_details(&mut repo.details, &combined_output(&output));
        }
        Ok(output) => {
            repo.status = "Build failed".to_owned();
            repo.failed = true;
            append_text_details(&mut repo.details, &combined_output(&output));
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
    repo
}

fn clone_repo(_owner: &str, mut repo: RepoResult) -> RepoResult {
    match fs::metadata(&repo.name) {
        Ok(_) => {
            repo.status = "Skipped".to_owned();
            return repo;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            repo.status = "Clone failed".to_owned();
            repo.failed = true;
            repo.details.push(format!(
                "inspect clone destination {:?}: {error}; verify permissions and retry",
                repo.name
            ));
            return repo;
        }
    }
    match ProcessCommand::new("git")
        .args(["clone", &repo.origin, &repo.name])
        .output()
    {
        Ok(output) if output.status.success() => {
            repo.status = "Cloned".to_owned();
            append_text_details(&mut repo.details, &combined_output(&output));
        }
        Ok(output) => {
            repo.status = "Clone failed".to_owned();
            repo.failed = true;
            append_text_details(&mut repo.details, &combined_output(&output));
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
    repo
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
    if !text.trim().is_empty() {
        details.extend(text.lines().map(str::to_owned));
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

fn column_widths(results: &[RepoResult]) -> ColumnWidths {
    ColumnWidths {
        repo: results
            .iter()
            .map(|result| result.name.chars().count())
            .max()
            .unwrap_or(0),
        origin: results
            .iter()
            .map(|result| result.origin.chars().count())
            .max()
            .unwrap_or(0),
    }
}

fn render_result(result: &RepoResult, color: ColorMode, widths: ColumnWidths) -> String {
    let mut stdout = String::new();
    stdout.push_str("==> ");
    stdout.push_str(&color.paint(YELLOW, &result.name));
    stdout.push_str(&" ".repeat(widths.repo - result.name.chars().count() + 4));
    stdout.push_str(&color.paint(YELLOW, &result.origin));
    stdout.push_str(&" ".repeat(widths.origin - result.origin.chars().count() + 4));
    stdout.push_str(&color.paint(YELLOW, &result.status));
    stdout.push('\n');
    for detail in &result.details {
        stdout.push_str("    ");
        stdout.push_str(detail);
        stdout.push('\n');
    }
    stdout
}

fn emit(stdout: &mut impl Write, stderr: &mut impl Write, text: &str) -> Result<(), CliError> {
    if let Err(error) = stdout
        .write_all(text.as_bytes())
        .and_then(|_| stdout.flush())
    {
        let _ = writeln!(stderr, "write repoctl output: {error}");
        return Err(CliError::runtime(
            "write repoctl output: verify standard output is writable and retry",
        ));
    }
    Ok(())
}

fn help(color: ColorMode) -> String {
    let name = color.paint("38;5;15", PROGRAM_NAME);
    format!(
        "{name} v{PROGRAM_VERSION}\n\
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

    #[test]
    fn terminal_summary_paints_all_three_values_yellow() {
        let results = vec![RepoResult {
            name: "bits".to_owned(),
            origin: "https://github.com/kquo/bits.git".to_owned(),
            status: "👍 main".to_owned(),
            details: Vec::new(),
            failed: false,
        }];
        let output = render_result(&results[0], ColorMode::new(true), column_widths(&results));
        assert!(output.contains("\x1b[38;5;226mbits\x1b[0m"));
        assert!(output.contains("\x1b[38;5;226mhttps://github.com/kquo/bits.git\x1b[0m"));
        assert!(output.contains("\x1b[38;5;226m👍 main\x1b[0m"));
    }
}
