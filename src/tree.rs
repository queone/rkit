//! Directory-tree rendering and command-line behavior for the `tree` binary.

use crate::color::ColorMode;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

const PROGRAM_NAME: &str = "tree";
pub const PROGRAM_VERSION: &str = "1.4.0";
const GREEN: &str = "38;5;46";
const BLUE: &str = "38;5;21";
const CYAN: &str = "38;5;51";
const WHITE: &str = "38;5;15";

/// A command-line or filesystem failure with its process exit code.
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

    /// Returns the diagnostic text intended for standard error.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the process exit code associated with this failure.
    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
}

/// Complete process output for a successful command.
#[derive(Debug, Eq, PartialEq)]
pub struct RunOutput {
    stdout: String,
    stderr: String,
}

impl RunOutput {
    fn new(stdout: String, warnings: Vec<String>) -> Self {
        let stderr = if warnings.is_empty() {
            String::new()
        } else {
            format!("{}\n", warnings.join("\n"))
        };
        Self { stdout, stderr }
    }

    /// Returns successful standard output.
    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    /// Returns non-fatal warnings intended for standard error.
    pub fn stderr(&self) -> &str {
        &self.stderr
    }
}

#[derive(Debug, Eq, PartialEq)]
enum Command {
    Help,
    Version,
    Tree {
        root: PathBuf,
        show_joined_path: bool,
    },
}

#[derive(Clone, Debug)]
struct Node {
    name: OsString,
    is_dir: bool,
}

trait DirectorySource {
    fn entries(&self, path: &Path) -> io::Result<Vec<Node>>;
}

struct Filesystem;

impl DirectorySource for Filesystem {
    fn entries(&self, path: &Path) -> io::Result<Vec<Node>> {
        fs::read_dir(path)?
            .map(|result| {
                let entry = result?;
                let file_type = entry.file_type()?;
                Ok(Node {
                    name: entry.file_name(),
                    is_dir: file_type.is_dir() && !file_type.is_symlink(),
                })
            })
            .collect()
    }
}

#[derive(Debug)]
struct Entry {
    prefix: String,
    is_last: bool,
    name: String,
    joined_path: PathBuf,
    is_dir: bool,
    scalar_length: usize,
}

/// Runs the tree command for the provided argument sequence.
///
/// Successful output is returned as a complete string so traversal failures
/// cannot leak a partial tree to standard output.
pub fn run<I, S>(args: I) -> Result<RunOutput, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    run_with(args, &Filesystem, ColorMode::detect_stdout())
}

fn run_with<I, S, D>(args: I, source: &D, color: ColorMode) -> Result<RunOutput, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    D: DirectorySource,
{
    match parse_args(args)? {
        Command::Help => Ok(RunOutput::new(help(color), Vec::new())),
        Command::Version => Ok(RunOutput::new(
            format!("{PROGRAM_NAME} v{PROGRAM_VERSION}\n"),
            Vec::new(),
        )),
        Command::Tree {
            root,
            show_joined_path,
        } => render_tree(source, &root, show_joined_path, color),
    }
}

fn parse_args<I, S>(args: I) -> Result<Command, CliError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut root = PathBuf::from(".");
    let mut show_joined_path = false;
    let mut parse_options = true;

    for value in args {
        let argument = value.into();
        if parse_options {
            match argument.to_str() {
                Some("-h" | "-?" | "--help") => return Ok(Command::Help),
                Some("-v" | "--version") => return Ok(Command::Version),
                Some("-f" | "--full-path") => {
                    show_joined_path = true;
                    continue;
                }
                Some("--") => {
                    parse_options = false;
                    continue;
                }
                Some(text) if text.starts_with('-') && text != "-" => {
                    return Err(CliError::usage(format!(
                        "parse option {text:?}: unsupported option; use --help for usage"
                    )));
                }
                _ => {}
            }
        }
        root = PathBuf::from(argument);
    }

    Ok(Command::Tree {
        root,
        show_joined_path,
    })
}

fn help(color: ColorMode) -> String {
    let name = color.paint(WHITE, PROGRAM_NAME);
    let usage = color.paint(WHITE, "Usage");
    let options = color.paint(WHITE, "Options");
    let examples = color.paint(WHITE, "Examples");
    format!(
        "{name} v{}\n\
Directory tree printer — https://github.com/queone/rkit\n\
{usage}\n\
  {name} [options] [directory]\n\
\n\
  Options can appear before or after directory operands. The last directory\n\
  operand is used. Use -- before a directory whose name begins with a dash.\n\
\n\
{options}\n\
  -f, --full-path  Show each file's path joined to the directory operand\n\
  -v, --version    Print version and exit\n\
  -h, -?, --help   Show this help message and exit\n\
  --               End option parsing\n\
\n\
{examples}\n\
  {name}\n\
  {name} -f /path/to/directory\n\
  {name} /path/to/directory --full-path\n\
  {name} -- -directory\n",
        PROGRAM_VERSION
    )
}

fn render_tree<D: DirectorySource>(
    source: &D,
    root: &Path,
    show_joined_path: bool,
    color: ColorMode,
) -> Result<RunOutput, CliError> {
    if root.to_str().is_none() {
        return Err(CliError::runtime(
            "read root operand: path is not valid UTF-8; rename the directory and retry",
        ));
    }
    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    gather(source, root, "", &mut entries, &mut warnings, true)?;
    let max_length = entries
        .iter()
        .map(|entry| entry.scalar_length)
        .max()
        .unwrap_or(0);
    let mut output = String::new();

    for entry in entries {
        let mark = if entry.is_last {
            "└── "
        } else {
            "├── "
        };
        output.push_str(&entry.prefix);
        output.push_str(mark);
        output.push_str(&color.paint(if entry.is_dir { BLUE } else { GREEN }, &entry.name));
        if show_joined_path && !entry.is_dir {
            let spacing = (max_length + 4).saturating_sub(entry.scalar_length).max(1);
            output.push_str(&" ".repeat(spacing));
            output.push_str(&color.paint(CYAN, entry.joined_path.to_string_lossy().as_ref()));
        }
        output.push('\n');
    }

    Ok(RunOutput::new(output, warnings))
}

fn gather<D: DirectorySource>(
    source: &D,
    directory: &Path,
    prefix: &str,
    output: &mut Vec<Entry>,
    warnings: &mut Vec<String>,
    is_root: bool,
) -> Result<(), CliError> {
    let mut nodes = match source.entries(directory) {
        Ok(nodes) => nodes,
        Err(error) if !is_root => {
            warnings.push(format!(
                "skip unreadable directory {}: {error}; grant access to include its contents",
                quoted_path(directory)
            ));
            return Ok(());
        }
        Err(error) => {
            return Err(CliError::runtime(format!(
                "read directory {}: {error}; verify the path exists and is readable",
                quoted_path(directory)
            )));
        }
    };

    let mut named = Vec::with_capacity(nodes.len());
    for node in nodes.drain(..) {
        let name = node.name.into_string().map_err(|_| {
            CliError::runtime(format!(
                "read directory entry in {}: filename is not valid UTF-8; rename the entry and retry",
                quoted_path(directory)
            ))
        })?;
        named.push((name, node.is_dir));
    }
    named.sort_by(|left, right| left.0.cmp(&right.0));

    let count = named.len();
    for (index, (name, is_dir)) in named.into_iter().enumerate() {
        let is_last = index + 1 == count;
        let mark = if is_last { "└── " } else { "├── " };
        let raw_line = format!("{prefix}{mark}{name}");
        let joined_path = clean_path(&directory.join(&name));
        output.push(Entry {
            prefix: prefix.to_owned(),
            is_last,
            name,
            joined_path: joined_path.clone(),
            is_dir,
            scalar_length: raw_line.chars().count(),
        });
        if is_dir {
            let next_prefix = format!("{prefix}{}", if is_last { "    " } else { "│   " });
            gather(source, &joined_path, &next_prefix, output, warnings, false)?;
        }
    }
    Ok(())
}

fn clean_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    let mut normal_components: Vec<OsString> = Vec::new();
    let mut rooted = false;

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::RootDir => {
                result.push(component.as_os_str());
                rooted = true;
                normal_components.clear();
            }
            Component::CurDir => {}
            Component::ParentDir => match normal_components.last() {
                Some(last) if last != OsStr::new("..") => {
                    normal_components.pop();
                }
                _ if !rooted => normal_components.push(OsString::from("..")),
                _ => {}
            },
            Component::Normal(value) => normal_components.push(value.to_owned()),
        }
    }
    for component in normal_components {
        result.push(component);
    }
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    result
}

fn quoted_path(path: &Path) -> String {
    format!("{:?}", path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MemorySource {
        entries: HashMap<PathBuf, io::Result<Vec<Node>>>,
    }

    impl DirectorySource for MemorySource {
        fn entries(&self, path: &Path) -> io::Result<Vec<Node>> {
            match self.entries.get(path) {
                Some(Ok(nodes)) => Ok(nodes.clone()),
                Some(Err(error)) => Err(io::Error::new(error.kind(), error.to_string())),
                None => Ok(Vec::new()),
            }
        }
    }

    fn node(name: &str, is_dir: bool) -> Node {
        Node {
            name: OsString::from(name),
            is_dir,
        }
    }

    fn source(entries: impl IntoIterator<Item = (PathBuf, Vec<Node>)>) -> MemorySource {
        MemorySource {
            entries: entries
                .into_iter()
                .map(|(path, nodes)| (path, Ok(nodes)))
                .collect(),
        }
    }

    #[test]
    fn parses_last_directory_and_full_path_in_any_order() {
        assert_eq!(
            parse_args(["first", "-f", "second", "--full-path"]).unwrap(),
            Command::Tree {
                root: PathBuf::from("second"),
                show_joined_path: true
            }
        );
    }

    #[test]
    fn parses_dash_directory_after_option_terminator() {
        assert_eq!(
            parse_args(["-f", "--", "-first", "-last"]).unwrap(),
            Command::Tree {
                root: PathBuf::from("-last"),
                show_joined_path: true
            }
        );
    }

    #[test]
    fn terminal_flag_ignores_later_arguments() {
        assert_eq!(
            parse_args(["root", "--help", "--bad"]).unwrap(),
            Command::Help
        );
        assert_eq!(
            parse_args(["root", "--version", "--bad"]).unwrap(),
            Command::Version
        );
        assert!(parse_args(["--bad", "--help"]).is_err());
    }

    #[test]
    fn unsupported_option_is_usage_error() {
        let error = parse_args(["--bad"]).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.message().contains("use --help"));
    }

    #[test]
    fn renders_empty_root_without_output() {
        let result = run_with(
            std::iter::empty::<&str>(),
            &source([(PathBuf::from("."), Vec::new())]),
            ColorMode::new(false),
        )
        .unwrap();
        assert_eq!(result.stdout(), "");
        assert_eq!(result.stderr(), "");
    }

    #[test]
    fn renders_sorted_nested_tree_with_dotfiles_and_symlink_as_file() {
        let filesystem = source([
            (
                PathBuf::from("."),
                vec![
                    node("βeta.txt", false),
                    node("nested", true),
                    node(".hidden", false),
                    node("alpha.txt", false),
                    node("link", false),
                ],
            ),
            (
                PathBuf::from("nested"),
                vec![node("wide界.txt", false), node("z.txt", false)],
            ),
        ]);
        let result = run_with(
            std::iter::empty::<&str>(),
            &filesystem,
            ColorMode::new(false),
        )
        .unwrap();
        assert_eq!(
            result.stdout(),
            "├── .hidden\n├── alpha.txt\n├── link\n├── nested\n\
             │   ├── wide界.txt\n│   └── z.txt\n└── βeta.txt\n"
                .replace("             ", "")
        );
        assert_eq!(result.stderr(), "");
    }

    #[test]
    fn aligns_joined_paths_by_unicode_scalar_count() {
        let filesystem = source([(
            PathBuf::from("."),
            vec![node("a.txt", false), node("界.txt", false)],
        )]);
        let result = run_with(["-f"], &filesystem, ColorMode::new(false)).unwrap();
        assert_eq!(
            result.stdout(),
            "├── a.txt    a.txt\n└── 界.txt    界.txt\n"
        );
    }

    #[test]
    fn cleans_relative_and_absolute_joined_paths() {
        assert_eq!(clean_path(Path::new("./a/../file")), PathBuf::from("file"));
        assert_eq!(
            clean_path(Path::new("/tmp/a/../file")),
            PathBuf::from("/tmp/file")
        );
    }

    #[test]
    fn renders_exact_colors_when_forced() {
        let filesystem = source([
            (
                PathBuf::from("."),
                vec![node("dir", true), node("file", false)],
            ),
            (PathBuf::from("dir"), Vec::new()),
        ]);
        let result = run_with(["-f"], &filesystem, ColorMode::new(true)).unwrap();
        assert!(result.stdout().contains("\x1b[38;5;21mdir\x1b[0m"));
        assert!(result.stdout().contains("\x1b[38;5;46mfile\x1b[0m"));
        assert!(result.stdout().contains("\x1b[38;5;51mfile\x1b[0m"));
        let help = run_with(["--help"], &filesystem, ColorMode::new(true)).unwrap();
        assert!(help.stdout().contains("\x1b[38;5;15mUsage\x1b[0m"));
    }

    #[test]
    fn suppresses_colors_when_disabled() {
        let filesystem = source([(PathBuf::from("."), vec![node("file", false)])]);
        let result = run_with(
            std::iter::empty::<&str>(),
            &filesystem,
            ColorMode::new(false),
        )
        .unwrap();
        assert!(!result.stdout().contains('\x1b'));
    }

    #[test]
    fn reports_utility_version() {
        let filesystem = source([]);
        let result = run_with(["--version"], &filesystem, ColorMode::new(false)).unwrap();
        assert_eq!(result.stdout(), format!("tree v{PROGRAM_VERSION}\n"));
    }

    #[test]
    fn descendant_failure_warns_and_continues_rendering() {
        let mut entries = HashMap::new();
        entries.insert(
            PathBuf::from("."),
            Ok(vec![
                node("before.txt", false),
                node("nested", true),
                node("z-after", true),
            ]),
        );
        entries.insert(
            PathBuf::from("nested"),
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "denied")),
        );
        entries.insert(PathBuf::from("z-after"), Ok(vec![node("child.txt", false)]));
        let filesystem = MemorySource { entries };
        let result = run_with(
            std::iter::empty::<&str>(),
            &filesystem,
            ColorMode::new(false),
        )
        .unwrap();
        assert_eq!(
            result.stdout(),
            "├── before.txt\n├── nested\n└── z-after\n    └── child.txt\n"
        );
        assert!(
            result
                .stderr()
                .contains("skip unreadable directory \"nested\"")
        );
        assert!(result.stderr().contains("grant access"));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_name_returns_runtime_error_without_output() {
        use std::os::unix::ffi::OsStringExt;

        let filesystem = source([(
            PathBuf::from("."),
            vec![Node {
                name: OsString::from_vec(vec![b'f', 0x80]),
                is_dir: false,
            }],
        )]);
        let error = run_with(
            std::iter::empty::<&str>(),
            &filesystem,
            ColorMode::new(false),
        )
        .unwrap_err();
        assert_eq!(error.exit_code(), 1);
        assert!(error.message().contains("filename is not valid UTF-8"));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_root_returns_runtime_error_without_output() {
        use std::os::unix::ffi::OsStringExt;

        let root = OsString::from_vec(vec![b'r', 0x80]);
        let error = run_with([root], &source([]), ColorMode::new(false)).unwrap_err();
        assert_eq!(error.exit_code(), 1);
        assert!(error.message().contains("root operand"));
    }
}
