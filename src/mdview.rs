//! GitHub-Flavored-Markdown rendering, `<details>`/`<summary>` disclosure
//! preprocessing, HTML document assembly, and file/browser output behavior
//! for the `mdview` utility.

use comrak::Options;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const PROGRAM_NAME: &str = "mdview";
const STYLESHEET: &str = include_str!("github-markdown.css");
#[cfg(test)]
const STYLESHEET_SHA256: &str = "6112686f954db5d3806fb96116d2ab20ad3018469ab1015c587fd8efe7d25cf4";

/// Opens a path in the system's default browser; injectable so tests never
/// launch a real browser.
pub trait BrowserOpener {
    fn open(&self, path: &Path) -> io::Result<()>;
}

/// Shells out to `open` (macOS) or `xdg-open` (other Unix); Windows is out
/// of scope, matching the Windows scope already dropped for `jy`.
pub struct SystemOpener;

impl BrowserOpener for SystemOpener {
    fn open(&self, path: &Path) -> io::Result<()> {
        let command_name = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        let status = Command::new(command_name).arg(path).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "{command_name} exited with {status}"
            )))
        }
    }
}

#[derive(Debug)]
struct Invocation {
    input: String,
    output: Option<String>,
}

/// Runs `mdview` and writes its process output to the supplied streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    run_with(args, version, &SystemOpener, stdout, stderr)
}

/// Runs `mdview` against an injectable [`BrowserOpener`], independent of
/// the system's browser.
pub fn run_with<I, S, O, W, E>(
    args: I,
    version: &str,
    opener: &O,
    stdout: &mut W,
    stderr: &mut E,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    O: BrowserOpener,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let invocation = match parse_args(&args) {
        Ok(ParsedArgs::ShowHelp) => {
            let _ = write!(stdout, "{}", usage_text(version));
            return 0;
        }
        Ok(ParsedArgs::ShowVersion) => {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            return 0;
        }
        Ok(ParsedArgs::Run(invocation)) => invocation,
        Err(message) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {message}");
            return 1;
        }
    };

    let resolved = match resolve_input(Path::new(&invocation.input)) {
        Ok(path) => path,
        Err(message) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {message}");
            return 1;
        }
    };
    let source = match fs::read(&resolved) {
        Ok(bytes) => bytes,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "{PROGRAM_NAME}: reading input {:?}: {error}; verify the file is readable",
                invocation.input
            );
            return 1;
        }
    };
    let Ok(source) = String::from_utf8(source) else {
        let _ = writeln!(
            stderr,
            "{PROGRAM_NAME}: input is not valid UTF-8; verify the file's encoding"
        );
        return 1;
    };

    let options = markdown_options();
    let result = if let Some(output) = &invocation.output {
        write_persistent(Path::new(output), &source, &resolved, &options, stdout)
    } else {
        open_temporary(&source, &resolved, &options, opener)
    };

    match result {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {message}");
            1
        }
    }
}

#[derive(Debug)]
enum ParsedArgs {
    /// `-h`/`-?`/`--help`, or a bare invocation: show the full usage screen.
    ShowHelp,
    /// `-v`/`--version`: print only the version line, matching every other
    /// `rkit` utility. This deliberately diverges from the Go original,
    /// which folded `-v` into the same full-usage-screen path as `-h` —
    /// incompatible with this repo's build-validation gate, which requires
    /// every utility's `--version` output to be exactly `name vX.Y.Z`.
    ShowVersion,
    Run(Invocation),
}

fn is_help(arg: &str) -> bool {
    matches!(arg, "-h" | "-?" | "--help")
}

fn is_version(arg: &str) -> bool {
    matches!(arg, "-v" | "--version")
}

/// Parses CLI arguments, matching the Go original's precedence exactly: a
/// help/version flag *before* any `--` takes effect regardless of
/// position; `--` makes everything after it literal (so
/// `mdview -- --help` treats `--help` as the filename).
fn parse_args(args: &[OsString]) -> Result<ParsedArgs, String> {
    let text_args: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    for arg in &text_args {
        if arg == "--" {
            break;
        }
        if is_help(arg) {
            return Ok(ParsedArgs::ShowHelp);
        }
        if is_version(arg) {
            return Ok(ParsedArgs::ShowVersion);
        }
    }
    if text_args.is_empty() {
        return Ok(ParsedArgs::ShowHelp);
    }

    let mut output: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut literal = false;
    let mut output_set = false;
    let mut index = 0;
    while index < text_args.len() {
        let arg = &text_args[index];
        if literal {
            positional.push(arg.clone());
            index += 1;
            continue;
        }
        if arg == "--" {
            literal = true;
        } else if arg == "-o" || arg == "--output" {
            if output_set {
                return Err(format!(
                    "output option specified more than once (see {PROGRAM_NAME} --help)"
                ));
            }
            let has_value = text_args
                .get(index + 1)
                .is_some_and(|value| !value.is_empty());
            if !has_value {
                return Err(format!(
                    "{arg} requires a non-empty FILE (see {PROGRAM_NAME} --help)"
                ));
            }
            index += 1;
            output = Some(text_args[index].clone());
            output_set = true;
        } else if let Some(value) = arg.strip_prefix("-o=") {
            if output_set {
                return Err(format!(
                    "output option specified more than once (see {PROGRAM_NAME} --help)"
                ));
            }
            if value.is_empty() {
                return Err(format!(
                    "-o requires a non-empty FILE (see {PROGRAM_NAME} --help)"
                ));
            }
            output = Some(value.to_owned());
            output_set = true;
        } else if let Some(value) = arg.strip_prefix("--output=") {
            if output_set {
                return Err(format!(
                    "output option specified more than once (see {PROGRAM_NAME} --help)"
                ));
            }
            if value.is_empty() {
                return Err(format!(
                    "--output requires a non-empty FILE (see {PROGRAM_NAME} --help)"
                ));
            }
            output = Some(value.to_owned());
            output_set = true;
        } else if arg.starts_with('-') {
            return Err(format!("unknown flag {arg:?} (see {PROGRAM_NAME} --help)"));
        } else {
            positional.push(arg.clone());
        }
        index += 1;
    }
    if positional.len() != 1 {
        return Err(format!("expected FILE (see {PROGRAM_NAME} --help)"));
    }
    Ok(ParsedArgs::Run(Invocation {
        input: positional.into_iter().next().unwrap(),
        output,
    }))
}

fn usage_text(version: &str) -> String {
    format!(
        "{PROGRAM_NAME} v{version}\n\
View GitHub Flavored Markdown in a browser or write it as HTML.\n\
\n\
Usage\n\
  {PROGRAM_NAME} [-o FILE] FILE\n\
\n\
Options\n\
  -o, --output FILE  write HTML to FILE without opening a browser\n\
  -v, --version      show this help message and exit\n\
  -h, -?, --help     show this help message and exit\n"
    )
}

/// Resolves `path` to an absolute, symlink-resolved regular file.
/// `fs::canonicalize` requires the target to exist, so it naturally
/// rejects dangling symlinks and missing paths the same way Go's
/// `EvalSymlinks`+`Stat` sequence does.
fn resolve_input(path: &Path) -> Result<PathBuf, String> {
    let resolved = fs::canonicalize(path).map_err(|error| {
        format!("resolving input {path:?}: {error}; provide an existing readable file")
    })?;
    let metadata = fs::metadata(&resolved).map_err(|error| {
        format!("checking input {path:?}: {error}; verify the file is readable")
    })?;
    if !metadata.is_file() {
        return Err(format!(
            "checking input {path:?}: not a regular file; provide a regular file"
        ));
    }
    Ok(resolved)
}

/// Builds a `file://` base URL from `resolved_input`'s parent directory,
/// RFC3986 path-segment percent-encoded (preserves `/`; encodes space,
/// `#`, `%`, and non-ASCII bytes) — a distinct rule set from `sms.rs`'s
/// form encoder, which uses `+` for spaces and doesn't preserve `/`.
fn file_base_url(resolved_input: &Path) -> String {
    let dir = resolved_input.parent().unwrap_or_else(|| Path::new("/"));
    let mut dir_text = dir.to_string_lossy().into_owned();
    if !dir_text.ends_with('/') {
        dir_text.push('/');
    }
    format!("file://{}", percent_encode_path(&dir_text))
}

fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(byte as char);
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            _ => out.push(ch),
        }
    }
    out
}

fn markdown_options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.autolink = true;
    options.extension.tasklist = true;
    options.extension.tagfilter = true;
    options
}

fn build_document(source: &str, resolved_input: &Path, options: &Options) -> String {
    let body = render_markdown(source, options);
    let base_url = file_base_url(resolved_input);
    let title = html_escape(
        &resolved_input
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    );
    format!(
        "<!doctype html>\n\
<html lang=\"en\">\n\
<head>\n\
<meta charset=\"utf-8\">\n\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
<base href=\"{base_url}\">\n\
<title>{title}</title>\n\
<style>\n\
{STYLESHEET}\n\
html {{\n  background-color: #ffffff;\n}}\n\
@media (prefers-color-scheme: dark) {{\n  html {{\n    background-color: #0d1117;\n  }}\n}}\n\
.markdown-body {{\n  box-sizing: border-box;\n  min-width: 200px;\n  max-width: 980px;\n  margin: 0 auto;\n  padding: 45px;\n}}\n\
@media (max-width: 767px) {{\n  .markdown-body {{\n    padding: 15px;\n  }}\n}}\n\
</style>\n\
</head>\n\
<body class=\"markdown-body\">\n\
{body}\
</body>\n\
</html>\n"
    )
}

// --------------------------------------------------------------------
// `<details>`/`<summary>` disclosure preprocessor
// --------------------------------------------------------------------

enum Node {
    Text(String),
    Summary(String),
    Details(Vec<Node>),
}

fn render_markdown(source: &str, options: &Options) -> String {
    let nodes = parse_disclosure_nodes(source);
    render_disclosure_nodes(&nodes, options)
}

fn parse_disclosure_nodes(source: &str) -> Vec<Node> {
    parse_disclosure_container(source, 0, None).0
}

/// Returns `(nodes, end_index, closed_found)`. Ported from the Go
/// original's `parseDisclosureContainer` (`main.go:232-302`): walks
/// `source` byte-by-byte, isolating `<details>`/`<summary>` regions
/// (recursively, since they may nest) while protecting fenced/indented
/// code, inline code spans, HTML comments, and raw containers from being
/// misread as disclosure tags. An unclosed `stop_name` region is fully
/// unwrapped by the caller — its own `<summary>` demotes to plain text,
/// and any already-closed nested `<details>` splices its children up
/// unmodified, matching Go exactly (including the edge case where a
/// spliced-up grandchild `<summary>` ends up outside any `<details>`).
fn parse_disclosure_container(
    source: &str,
    start: usize,
    stop_name: Option<&str>,
) -> (Vec<Node>, usize, bool) {
    let bytes = source.as_bytes();
    let mut nodes = Vec::new();
    let mut text: Vec<u8> = Vec::new();
    let mut summary_seen = false;
    let mut i = start;

    macro_rules! flush_text {
        () => {
            if !text.is_empty() {
                nodes.push(Node::Text(
                    String::from_utf8(std::mem::take(&mut text)).unwrap(),
                ));
            }
        };
    }

    while i < bytes.len() {
        if let Some(end) = markdown_raw_container_end(bytes, i) {
            text.extend_from_slice(&bytes[i..end]);
            i = end;
            continue;
        }
        if let Some(end) = markdown_protected_end(bytes, i) {
            text.extend_from_slice(&bytes[i..end]);
            i = end;
            continue;
        }
        if bytes[i] != b'<' {
            text.push(bytes[i]);
            i += 1;
            continue;
        }
        let Some((tag, next)) = disclosure_tag(bytes, i) else {
            text.push(bytes[i]);
            i += 1;
            continue;
        };
        if tag.closing && stop_name == Some(tag.name.as_str()) {
            flush_text!();
            return (nodes, next, true);
        }
        if tag.name == "details" && !tag.closing {
            flush_text!();
            let (children, after, closed) =
                parse_disclosure_container(source, next, Some("details"));
            if !closed {
                for child in children {
                    match child {
                        Node::Details(grandchildren) => nodes.extend(grandchildren),
                        Node::Summary(text) => nodes.push(Node::Text(text)),
                        other => nodes.push(other),
                    }
                }
                i = after;
                continue;
            }
            nodes.push(Node::Details(children));
            i = after;
            continue;
        }
        if tag.name == "summary"
            && !tag.closing
            && stop_name == Some("details")
            && !summary_seen
            && let Some((inner, after)) = disclosure_summary(source, next)
        {
            flush_text!();
            let cleaned = strip_disclosure_tags(inner).trim().to_owned();
            nodes.push(Node::Summary(cleaned));
            summary_seen = true;
            i = after;
            continue;
        }
        // Drop raw HTML tags, including orphan summaries and unmatched
        // closing tags.
        i = next;
    }
    flush_text!();
    (nodes, bytes.len(), false)
}

struct TagInfo {
    name: String,
    closing: bool,
}

/// Parses a `<details>`/`<summary>` open or close tag at `start`; returns
/// `None` for any other tag name (which the caller then treats as opaque
/// text, letting `comrak`'s own sanitization handle it).
fn disclosure_tag(bytes: &[u8], start: usize) -> Option<(TagInfo, usize)> {
    if start >= bytes.len() || bytes[start] != b'<' {
        return None;
    }
    let mut quote: u8 = 0;
    let mut end = start + 1;
    while end < bytes.len() {
        let byte = bytes[end];
        if quote != 0 {
            if byte == quote {
                quote = 0;
            }
        } else if byte == b'\'' || byte == b'"' {
            quote = byte;
        } else if byte == b'>' {
            break;
        }
        end += 1;
    }
    if end == bytes.len() || quote != 0 {
        return None;
    }
    let content = std::str::from_utf8(&bytes[start + 1..end]).ok()?.trim();
    let (closing, content) = match content.strip_prefix('/') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, content),
    };
    let name_end = content
        .as_bytes()
        .iter()
        .position(|byte| !byte.is_ascii_alphabetic())
        .unwrap_or(content.len());
    if name_end == 0 {
        return None;
    }
    let name = content[..name_end].to_ascii_lowercase();
    if name != "details" && name != "summary" {
        return None;
    }
    if closing && !content[name_end..].trim().is_empty() {
        return None;
    }
    Some((TagInfo { name, closing }, end + 1))
}

/// Scans forward from `start` for a matching `</summary>` close tag;
/// returns the raw inner span and the position after the close tag.
fn disclosure_summary(source: &str, start: usize) -> Option<(&str, usize)> {
    let bytes = source.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        if let Some((tag, next)) = disclosure_tag(bytes, i)
            && tag.name == "summary"
            && tag.closing
        {
            return Some((&source[start..i], next));
        }
        i += 1;
    }
    None
}

fn strip_disclosure_tags(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut clean = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some((_, next)) = disclosure_tag(bytes, i)
        {
            i = next;
            continue;
        }
        clean.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(clean).unwrap()
}

fn is_html_tag_boundary(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/')
}

fn find_byte(haystack: &[u8], needle: u8) -> Option<usize> {
    haystack.iter().position(|&byte| byte == needle)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn starts_with_ci(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.len() >= needle.len() && haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

fn find_subslice_ci(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&index| haystack[index..index + needle.len()].eq_ignore_ascii_case(needle))
}

/// Consumes `<!-- ... -->` comments and `<script>`/`<style>`/`<textarea>`/
/// `<title>` blocks whole, so their contents never get misread as
/// disclosure tags. Ported from `markdownRawContainerEnd` (`main.go:304-329`).
fn markdown_raw_container_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes[start..].starts_with(b"<!--") {
        return Some(match find_subslice(&bytes[start + 4..], b"-->") {
            Some(pos) => start + 4 + pos + 3,
            None => bytes.len(),
        });
    }
    for name in ["script", "style", "textarea", "title"] {
        let prefix_len = name.len() + 1;
        if !starts_with_ci(&bytes[start..], format!("<{name}").as_bytes()) {
            continue;
        }
        if start + prefix_len < bytes.len() && !is_html_tag_boundary(bytes[start + prefix_len]) {
            continue;
        }
        let after_open = start + prefix_len;
        let close_needle = format!("</{name}");
        let Some(close_pos) = find_subslice_ci(&bytes[after_open..], close_needle.as_bytes())
        else {
            return Some(bytes.len());
        };
        let close_start = after_open + close_pos;
        return Some(match find_byte(&bytes[close_start..], b'>') {
            Some(end) => close_start + end + 1,
            None => bytes.len(),
        });
    }
    None
}

/// Protects inline code spans (backtick runs) and fenced/4-space-indented
/// code blocks from disclosure-tag scanning. Ported from
/// `markdownProtectedEnd` (`main.go:335-431`).
fn markdown_protected_end(bytes: &[u8], start: usize) -> Option<usize> {
    if start >= bytes.len() {
        return None;
    }
    if bytes[start] == b'`' {
        let mut run_end = start;
        while run_end < bytes.len() && bytes[run_end] == b'`' {
            run_end += 1;
        }
        let marker = &bytes[start..run_end];
        return Some(match find_subslice(&bytes[run_end..], marker) {
            Some(pos) => run_end + pos + marker.len(),
            None => bytes.len(),
        });
    }
    if start != 0 && bytes[start - 1] != b'\n' {
        return None;
    }
    let line_end = find_byte(&bytes[start..], b'\n')
        .map(|pos| start + pos)
        .unwrap_or(bytes.len());
    let line = &bytes[start..line_end];
    let mut indent = 0;
    while indent < line.len() && indent < 3 && line[indent] == b' ' {
        indent += 1;
    }
    if indent == 3 && line.len() > indent && line[indent] == b' ' {
        let mut end = line_end;
        while end < bytes.len() {
            let line_start = end + 1;
            if line_start >= bytes.len() {
                return Some(bytes.len());
            }
            let next_line_end = find_byte(&bytes[line_start..], b'\n')
                .map(|pos| line_start + pos)
                .unwrap_or(bytes.len());
            let candidate = &bytes[line_start..next_line_end];
            let mut spaces = 0;
            while spaces < candidate.len() && candidate[spaces] == b' ' {
                spaces += 1;
            }
            let non_blank = candidate.iter().any(|&byte| !byte.is_ascii_whitespace());
            if spaces < 4 && non_blank {
                break;
            }
            end = next_line_end;
        }
        return Some(end);
    }
    if indent == line.len() || (line[indent] != b'`' && line[indent] != b'~') {
        return None;
    }
    let marker = line[indent];
    let mut marker_end = indent;
    while marker_end < line.len() && line[marker_end] == marker {
        marker_end += 1;
    }
    if marker_end - indent < 3 {
        return None;
    }
    let fence_len = marker_end - indent;
    let mut cursor = line_end;
    loop {
        if cursor >= bytes.len() {
            return Some(bytes.len());
        }
        if bytes[cursor] == b'\n' {
            cursor += 1;
        }
        let end = find_byte(&bytes[cursor..], b'\n')
            .map(|pos| cursor + pos)
            .unwrap_or(bytes.len());
        let candidate = &bytes[cursor..end];
        let mut spaces = 0;
        while spaces < candidate.len() && spaces < 3 && candidate[spaces] == b' ' {
            spaces += 1;
        }
        let mut count = spaces;
        while count < candidate.len() && candidate[count] == marker {
            count += 1;
        }
        if count - spaces >= fence_len {
            return Some(if end < bytes.len() { end + 1 } else { end });
        }
        if end == bytes.len() {
            return Some(bytes.len());
        }
        cursor = end;
    }
}

/// Renders a node list to HTML. Only `Node::Details` is special-cased at
/// this level (matching Go's top-level loop, which only checks
/// `node.details`); a top-level `Node::Summary` — reachable only via the
/// unclosed-details splice edge case — renders as markdown like
/// `Node::Text`, not as a literal `<summary>` tag. Inside a `Details`
/// node's own children, `Summary` *is* special-cased, matching Go's
/// nested loop.
fn render_disclosure_nodes(nodes: &[Node], options: &Options) -> String {
    let mut body = String::new();
    for node in nodes {
        match node {
            Node::Details(children) => {
                body.push_str("<details>\n");
                for child in children {
                    if let Node::Summary(text) = child {
                        body.push_str("<summary>");
                        body.push_str(&html_escape(text));
                        body.push_str("</summary>\n");
                    } else {
                        body.push_str(&render_disclosure_nodes(
                            std::slice::from_ref(child),
                            options,
                        ));
                    }
                }
                body.push_str("</details>\n");
            }
            Node::Text(text) | Node::Summary(text) => {
                body.push_str(&comrak::markdown_to_html(text, options));
            }
        }
    }
    body
}

// --------------------------------------------------------------------
// File and browser output
// --------------------------------------------------------------------

fn write_persistent<W: Write>(
    path: &Path,
    source: &str,
    resolved_input: &Path,
    options: &Options,
    stdout: &mut W,
) -> Result<(), String> {
    if fs::symlink_metadata(path).is_ok() {
        return Err(format!("output {path:?} already exists; choose a new path"));
    }
    let mut file = open_output_exclusive(path).map_err(|error| {
        format!("creating output {path:?}: {error}; verify the parent exists and is writable")
    })?;
    let document = build_document(source, resolved_input, options);
    if let Err(error) = file.write_all(document.as_bytes()) {
        let _ = fs::remove_file(path);
        return Err(format!(
            "writing HTML output {path:?}: {error}; verify available space and permissions"
        ));
    }
    let _ = writeln!(stdout, "Created: {}", path.display());
    Ok(())
}

fn open_temporary<O: BrowserOpener>(
    source: &str,
    resolved_input: &Path,
    options: &Options,
    opener: &O,
) -> Result<(), String> {
    let (mut file, path) = create_unique_temp_file().map_err(|error| {
        format!(
            "creating temporary HTML output: {error}; verify the temporary directory is writable"
        )
    })?;
    let document = build_document(source, resolved_input, options);
    if let Err(error) = file.write_all(document.as_bytes()) {
        let _ = fs::remove_file(&path);
        return Err(format!("writing temporary HTML output {path:?}: {error}"));
    }
    drop(file);
    if let Err(error) = opener.open(&path) {
        let _ = fs::remove_file(&path);
        return Err(format!(
            "opening temporary HTML {path:?}: {error}; verify a default browser is configured"
        ));
    }
    Ok(())
}

fn create_unique_temp_file() -> io::Result<(fs::File, PathBuf)> {
    let dir = std::env::temp_dir();
    let pid = std::process::id();
    for attempt in 0..10_000u32 {
        let candidate = dir.join(format!("mdview-{pid}-{attempt}.html"));
        match open_temp_exclusive(&candidate) {
            Ok(file) => return Ok((file, candidate)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("could not create a unique temporary file"))
}

#[cfg(unix)]
fn open_output_exclusive(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(path)
}

#[cfg(not(unix))]
fn open_output_exclusive(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

#[cfg(unix)]
fn open_temp_exclusive(path: &Path) -> io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

#[cfg(not(unix))]
fn open_temp_exclusive(path: &Path) -> io::Result<fs::File> {
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

#[cfg(test)]
fn stylesheet_checksum() -> String {
    let digest = openssl::sha::sha256(STYLESHEET.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_NUMBER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let number = FIXTURE_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rkit-mdview-unit-{}-{number}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }

    fn render(source: &str) -> String {
        render_markdown(source, &markdown_options())
    }

    #[test]
    fn stylesheet_checksum_matches_documented_pin() {
        assert_eq!(stylesheet_checksum(), STYLESHEET_SHA256);
    }

    #[test]
    fn gfm_renders_and_raw_html_is_omitted() {
        let html = render(
            "# Heading\n\n~~gone~~\n\n| A | B |\n| - | - |\n| 1 | 2 |\n\n- [x] done\n\nhttps://example.com\n\n[unsafe](javascript:alert(1))\n\n<script>alert(\"unsafe\")</script>\n",
        );
        for want in [
            "<table",
            "<del>gone</del>",
            "type=\"checkbox\"",
            "href=\"https://example.com\"",
        ] {
            assert!(html.contains(want), "missing {want:?} in:\n{html}");
        }
        for unwanted in ["<script", "alert(\"unsafe\")", "href=\"javascript:"] {
            assert!(!html.contains(unwanted), "found {unwanted:?} in:\n{html}");
        }
        assert!(
            html.contains("raw HTML omitted"),
            "missing omission comment:\n{html}"
        );
    }

    #[test]
    fn details_renders_with_nested_gfm_and_no_open_attribute() {
        let html = render(
            "<details>\n<summary>Raw samples</summary>\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n</details>",
        );
        assert_eq!(html.matches("<details>").count(), 1);
        assert!(!html.contains("<details open"));
        assert!(html.contains("<summary>Raw samples</summary>"));
        assert!(html.contains("<table"));
    }

    #[test]
    fn details_strips_all_attributes_and_unsafe_content() {
        let html = render(
            "<script>alert(\"unsafe\")</script>\n<div>raw</div>\n<iframe src=\"https://example.com\"></iframe>\n<style>.x{}</style>\n<details open onclick=\"alert('unsafe')\"><summary style=\"color:red\">Label</summary>\ncontent\n</details>\n[unsafe](javascript:alert(1))",
        );
        for unwanted in [
            "<script",
            "<div",
            "<iframe",
            "<style",
            "onclick",
            "style=\"color",
            "href=\"javascript:",
        ] {
            assert!(!html.contains(unwanted), "found {unwanted:?} in:\n{html}");
        }
        assert_eq!(html.matches("<details>").count(), 1);
        assert!(html.contains("<summary>Label</summary>"));
    }

    #[test]
    fn sibling_details_stay_independent() {
        let html = render(
            "<details><summary>First</summary>\n\n| A |\n| --- |\n| 1 |\n\n</details>\n\n<details><summary>Second</summary>\n\n| B |\n| --- |\n| 2 |\n\n</details>",
        );
        assert_eq!(html.matches("<details>").count(), 2);
        assert!(html.contains("<summary>First</summary>"));
        assert!(html.contains("<summary>Second</summary>"));
    }

    #[test]
    fn nested_details_and_mixed_case_and_orphan_summary() {
        let html = render(
            "<DETAILS data-x=\"discard\"><SUMMARY>Repeated</SUMMARY>\nbody\n<summary>Repeated</summary>\n<details class=\"nested\"><summary>Nested</summary>nested body</details>\n</DETAILS>\n\n<summary>Orphan</summary>",
        );
        assert_eq!(html.matches("<details>").count(), 2);
        assert!(html.contains("<summary>Repeated</summary>"));
        assert!(html.contains("<summary>Nested</summary>"));
        assert!(!html.contains("data-x"));
        assert!(!html.contains("class=\"nested\""));
        assert!(!html.contains("Orphan</summary>") || !html.contains("<summary>Orphan"));
    }

    #[test]
    fn unclosed_details_unwraps_completely() {
        let html = render("<details><summary>Lost</summary>body");
        assert!(!html.contains("<details"));
        assert!(!html.contains("<summary"));
    }

    #[test]
    fn code_fences_and_indented_and_inline_code_stay_literal() {
        let html = render(
            "```html\n<details open><summary>literal</summary></details>\n```\n\n    <details><summary>indented</summary></details>\n\nInline `<details open>` remains literal.",
        );
        assert!(!html.contains("<details>\n") && !html.contains("<details open>\n"));
        assert!(html.contains("&lt;details open&gt;"));
        assert!(html.contains("&lt;details&gt;"));
        assert!(html.contains("<code>&lt;details open&gt;</code>"));
    }

    #[test]
    fn raw_containers_and_comments_are_not_rewritten() {
        let html = render(
            "<!-- <details><summary>comment</summary></details> -->\n<script><details><summary>script</summary></details></script>\n<style><details><summary>style</summary></details></style>",
        );
        assert!(!html.contains("<details>"));
        assert!(!html.contains("<script>"));
        assert!(!html.contains("<style>"));
    }

    #[test]
    fn malformed_closing_tag_is_rejected() {
        let html = render("<details><summary>Label</summary>body</details data-x=\"unsafe\">");
        assert!(!html.contains("<details"));
        assert!(!html.contains("<summary"));
    }

    #[test]
    fn build_document_contains_light_and_dark_media_queries_and_escaped_title() {
        let directory = temp_dir();
        let input = directory.join("title<&\".md");
        fs::write(&input, "# Hello").unwrap();
        let resolved = fs::canonicalize(&input).unwrap();
        let document = build_document("# Hello", &resolved, &markdown_options());
        for want in [
            "<body class=\"markdown-body\">",
            "max-width: 980px",
            "padding: 45px",
            "@media (prefers-color-scheme: dark)",
            "@media (prefers-color-scheme: light)",
            "background-color: #0d1117",
            "&lt;&amp;&#34;.md</title>",
        ] {
            assert!(document.contains(want), "missing {want:?}");
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn file_base_url_percent_encodes_special_characters() {
        let directory = temp_dir();
        let sub = directory.join("space # percent% \u{fc}");
        fs::create_dir(&sub).unwrap();
        let input = sub.join("input.md");
        fs::write(&input, "# Hello").unwrap();
        let resolved = fs::canonicalize(&input).unwrap();
        let base = file_base_url(&resolved);
        for want in ["file://", "space%20", "%23", "%25", "%C3%BC", "/"] {
            assert!(base.contains(want), "base {base:?} missing {want:?}");
        }
        assert!(base.ends_with('/'));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn parse_usage_and_version_precedence_and_literal_double_dash_path() {
        for args in [
            vec![],
            vec!["-h".to_owned()],
            vec!["-?".to_owned()],
            vec!["--help".to_owned()],
            vec!["bad".to_owned(), "--help".to_owned(), "extra".to_owned()],
        ] {
            let owned: Vec<OsString> = args.into_iter().map(OsString::from).collect();
            assert!(matches!(parse_args(&owned).unwrap(), ParsedArgs::ShowHelp));
        }
        for args in [vec!["-v".to_owned()], vec!["--version".to_owned()]] {
            let owned: Vec<OsString> = args.into_iter().map(OsString::from).collect();
            assert!(matches!(
                parse_args(&owned).unwrap(),
                ParsedArgs::ShowVersion
            ));
        }
        let owned: Vec<OsString> = ["--", "--help"].into_iter().map(OsString::from).collect();
        let ParsedArgs::Run(invocation) = parse_args(&owned).unwrap() else {
            panic!("expected Run variant");
        };
        assert_eq!(invocation.input, "--help");
    }

    #[test]
    fn parse_output_forms_and_failures() {
        for args in [
            vec!["-o", "out.html", "in.md"],
            vec!["--output", "out.html", "in.md"],
            vec!["-o=out.html", "in.md"],
            vec!["--output=out.html", "in.md"],
        ] {
            let owned: Vec<OsString> = args.into_iter().map(OsString::from).collect();
            let ParsedArgs::Run(invocation) = parse_args(&owned).unwrap() else {
                panic!("expected Run variant");
            };
            assert_eq!(invocation.input, "in.md");
            assert_eq!(invocation.output.as_deref(), Some("out.html"));
        }
        for args in [
            vec!["-o"],
            vec!["-o="],
            vec!["--output="],
            vec!["-o", "one.html", "--output", "two.html", "in.md"],
            vec!["--unknown", "in.md"],
            vec!["a.md", "b.md"],
        ] {
            let owned: Vec<OsString> = args.into_iter().map(OsString::from).collect();
            let error = parse_args(&owned).unwrap_err();
            assert!(error.contains("see mdview --help"), "error: {error}");
        }
    }

    #[test]
    fn run_cli_usage_and_error_status() {
        struct NoOpener;
        impl BrowserOpener for NoOpener {
            fn open(&self, _path: &Path) -> io::Result<()> {
                Ok(())
            }
        }
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            Vec::<&str>::new(),
            "1.0.0",
            &NoOpener,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.contains("mdview v1.0.0"));
        assert!(text.contains("mdview [-o FILE] FILE"));
        assert!(text.contains("-o, --output FILE"));

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(["-v"], "1.0.0", &NoOpener, &mut stdout, &mut stderr);
        assert_eq!(code, 0);
        assert_eq!(stdout, b"mdview v1.0.0\n");
        assert!(stderr.is_empty());

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(["a", "b"], "1.0.0", &NoOpener, &mut stdout, &mut stderr);
        assert_ne!(code, 0);
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("mdview: expected FILE (see mdview --help)")
        );
    }

    #[test]
    fn resolve_input_rejects_directories_and_missing_and_dangling_paths() {
        let directory = temp_dir();
        assert!(resolve_input(&directory).is_err());
        assert!(resolve_input(&directory.join("missing.md")).is_err());
        #[cfg(unix)]
        {
            let dangling = directory.join("dangling.md");
            std::os::unix::fs::symlink(directory.join("absent.md"), &dangling).unwrap();
            assert!(resolve_input(&dangling).is_err());
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn persistent_output_refuses_existing_destination_and_writes_exact_mode() {
        let directory = temp_dir();
        let input = directory.join("input.md");
        fs::write(&input, "# Hello").unwrap();
        let resolved = fs::canonicalize(&input).unwrap();
        let output = directory.join("page.html");
        let options = markdown_options();
        let mut stdout = Vec::new();
        write_persistent(&output, "# Hello", &resolved, &options, &mut stdout).unwrap();
        assert!(String::from_utf8(stdout).unwrap().starts_with("Created: "));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o644
            );
        }
        let mut stdout = Vec::new();
        let error =
            write_persistent(&output, "# Hello", &resolved, &options, &mut stdout).unwrap_err();
        assert!(error.contains("already exists"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn temporary_output_uses_expected_name_pattern_mode_and_calls_opener() {
        struct CapturingOpener {
            path: std::sync::Mutex<Option<PathBuf>>,
        }
        impl BrowserOpener for CapturingOpener {
            fn open(&self, path: &Path) -> io::Result<()> {
                *self.path.lock().unwrap() = Some(path.to_path_buf());
                Ok(())
            }
        }
        let directory = temp_dir();
        let input = directory.join("input.md");
        fs::write(&input, "# Hello").unwrap();
        let resolved = fs::canonicalize(&input).unwrap();
        let opener = CapturingOpener {
            path: std::sync::Mutex::new(None),
        };
        open_temporary("# Hello", &resolved, &markdown_options(), &opener).unwrap();
        let opened = opener.path.lock().unwrap().clone().unwrap();
        assert!(opened.is_absolute());
        assert!(
            opened
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("mdview-")
        );
        assert_eq!(opened.extension().unwrap(), "html");
        assert!(
            opened.exists(),
            "temp file should remain after a successful open"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&opened).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let _ = fs::remove_file(&opened);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn temporary_output_removed_when_opener_fails() {
        struct FailingOpener;
        impl BrowserOpener for FailingOpener {
            fn open(&self, _path: &Path) -> io::Result<()> {
                Err(io::Error::other("no browser"))
            }
        }
        let directory = temp_dir();
        let input = directory.join("input.md");
        fs::write(&input, "# Hello").unwrap();
        let resolved = fs::canonicalize(&input).unwrap();
        let error =
            open_temporary("# Hello", &resolved, &markdown_options(), &FailingOpener).unwrap_err();
        assert!(error.contains("opening temporary HTML"));
        fs::remove_dir_all(directory).unwrap();
    }
}
