//! DuckDuckGo search, HTML-result scraping, and browser-opening behavior
//! for the `web` utility.
//!
//! The default (non-`-j`, non-`--open`) path opens an interactive
//! fuzzy-finder picker via `nucleo-picker` when stdout/stdin are real
//! terminals, matching Go's unconditional `go-fzf` picker but with a
//! single-line title+snippet row in place of Go's split preview pane (see
//! `governa/ac35-web-picker.md`). A piped/scripted invocation falls back
//! to the numbered-list default instead, since an interactive picker
//! can't run there regardless of crate choice.

use crate::mdview::BrowserOpener;
use crate::pman::{HttpRequest, HttpResponse, HttpTransport};
use nucleo_picker::{Picker, Render};
use scraper::{Html, Selector};
use serde_json::{Map, Value};
use std::env;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, IsTerminal, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Display rows are truncated to this many characters of snippet text
/// (after collapsing whitespace) to keep each picker row to one line.
const SNIPPET_DISPLAY_CHARS: usize = 80;

const PROGRAM_NAME: &str = "web";
const DEFAULT_REFERRER: &str = "https://google.com";
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; WOW64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/113.0.5666.197 Safari/537.36";
const DEFAULT_TIMEOUT_SECS: u64 = 5;
const SEARCH_HOST: &str = "html.duckduckgo.com";

#[derive(Clone, Debug, PartialEq)]
pub struct SearchResult {
    pub title: String,
    pub link: String,
    pub snippet: String,
}

enum Action {
    ShowHelp,
    ShowVersion,
    Run(ParsedArgs),
}

#[derive(Default)]
struct ParsedArgs {
    json: bool,
    timeout: Option<u64>,
    user_agent: Option<String>,
    referrer: Option<String>,
    browser: Option<String>,
    open: Option<usize>,
    query_words: Vec<String>,
}

/// Opens a path or URL via a fixed external command (the `-b`/`--browser`
/// override); implements the shared [`BrowserOpener`] trait from
/// `mdview.rs` for a locally-defined type.
struct CommandOpener {
    command: String,
}

impl BrowserOpener for CommandOpener {
    fn open(&self, path: &Path) -> io::Result<()> {
        let status = Command::new(&self.command).arg(path).status()?;
        if status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "{} exited with {status}",
                self.command
            )))
        }
    }
}

/// Drives an interactive result picker; injectable so no test drives a
/// real interactive terminal session.
pub trait ResultPicker {
    /// Returns the selected result, or `None` when the user cancelled
    /// without selecting (matching Go's silent-return-on-empty-selection
    /// behavior).
    fn pick(&self, results: &[SearchResult]) -> io::Result<Option<SearchResult>>;
}

/// Renders each result as a single `<title>  <truncated snippet>` line —
/// `nucleo-picker`'s `Render` trait has no split-preview-pane mechanism
/// (confirmed against its docs during this AC's Refine), unlike Go's
/// `go-fzf` `WithPreviewWindow`. Fuzzy matching runs against this same
/// rendered string, so unlike Go (title-only matching) the snippet text is
/// also searchable — a disclosed, low-stakes side effect of the row format
/// choice, not a separate decision.
struct ResultRenderer;

impl Render<SearchResult> for ResultRenderer {
    type Str<'a> = String;

    fn render(&self, value: &SearchResult) -> String {
        format!(
            "{}  {}",
            value.title,
            truncate_snippet(&value.snippet, SNIPPET_DISPLAY_CHARS)
        )
    }
}

/// Collapses internal whitespace (scraped snippets can contain newlines)
/// and truncates to `max_chars`, appending an ellipsis when truncated.
fn truncate_snippet(snippet: &str, max_chars: usize) -> String {
    let collapsed = snippet.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        collapsed
    } else {
        let truncated: String = collapsed.chars().take(max_chars).collect();
        format!("{truncated}…")
    }
}

/// The real interactive picker, backed by `nucleo-picker`.
pub struct InteractivePicker;

impl ResultPicker for InteractivePicker {
    fn pick(&self, results: &[SearchResult]) -> io::Result<Option<SearchResult>> {
        if results.is_empty() {
            return Ok(None);
        }
        let mut picker = Picker::new(ResultRenderer);
        let injector = picker.injector();
        for result in results {
            injector.push(result.clone());
        }
        picker
            .pick()
            .map(|selection| selection.cloned())
            .map_err(io::Error::other)
    }
}

/// Minimal HTTPS/1.1 client with a configurable connect/read/write
/// timeout, driving the shared `pman` [`HttpRequest`]/[`HttpResponse`]
/// types. TCP timeout and response parsing stay local while TLS trust is shared.
struct TimeoutHttpTransport {
    timeout: Duration,
}

impl HttpTransport for TimeoutHttpTransport {
    fn send(&self, request: &HttpRequest) -> io::Result<HttpResponse> {
        let (host, path) = split_https_url(&request.url).map_err(io::Error::other)?;

        let mut header_bytes = Vec::new();
        write!(header_bytes, "{} {} HTTP/1.1\r\n", request.method, path)?;
        write!(header_bytes, "Host: {host}\r\n")?;
        for (name, value) in &request.headers {
            write!(header_bytes, "{name}: {value}\r\n")?;
        }
        if let Some(body) = &request.body {
            write!(header_bytes, "Content-Length: {}\r\n", body.len())?;
        }
        write!(header_bytes, "Connection: close\r\n\r\n")?;

        let address = format!("{host}:443");
        let addr = address
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::other(format!("could not resolve {host}")))?;
        let stream = TcpStream::connect_timeout(&addr, self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        let connector = crate::tls::connector().map_err(io::Error::other)?;
        let mut tls = connector
            .connect(&host, stream)
            .map_err(|error| io::Error::other(error.to_string()))?;
        tls.write_all(&header_bytes)?;
        if let Some(body) = &request.body {
            tls.write_all(body)?;
        }
        read_response(&mut tls)
    }
}

fn split_https_url(url: &str) -> Result<(String, String), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| "only https URLs are supported".to_owned())?;
    match rest.find('/') {
        Some(index) => Ok((rest[..index].to_owned(), rest[index..].to_owned())),
        None => Ok((rest.to_owned(), "/".to_owned())),
    }
}

fn read_response<R: Read>(reader: &mut R) -> io::Result<HttpResponse> {
    let mut buffered = BufReader::new(reader);
    let mut status_line = String::new();
    buffered.read_line(&mut status_line)?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| io::Error::other(format!("malformed status line: {status_line:?}")))?;

    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    loop {
        let mut line = String::new();
        let read = buffered.read_line(&mut line)?;
        if read == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim();
            if name == "content-length" {
                content_length = value.parse::<usize>().ok();
            } else if name == "transfer-encoding" && value.eq_ignore_ascii_case("chunked") {
                chunked = true;
            }
        }
    }

    let body = if chunked {
        read_chunked_body(&mut buffered)?
    } else if let Some(length) = content_length {
        let mut body = vec![0u8; length];
        buffered.read_exact(&mut body)?;
        body
    } else {
        let mut body = Vec::new();
        buffered.read_to_end(&mut body)?;
        body
    };

    Ok(HttpResponse { status, body })
}

fn read_chunked_body<R: BufRead>(reader: &mut R) -> io::Result<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let mut size_line = String::new();
        reader.read_line(&mut size_line)?;
        let size_text = size_line.trim().split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|error| io::Error::other(format!("malformed chunk size: {error}")))?;
        if size == 0 {
            let mut trailer = String::new();
            reader.read_line(&mut trailer)?;
            break;
        }
        let mut chunk = vec![0u8; size];
        reader.read_exact(&mut chunk)?;
        body.extend_from_slice(&chunk);
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf)?;
    }
    Ok(body)
}

/// Builds the search URL exactly as Go's `buildURL` does: alphabetically
/// sorted, form-urlencoded query params, matching Go's `TestBuildRequest`.
fn build_search_url(query: &str) -> String {
    let pairs = [
        ("api", "/d.js"),
        ("dc", "1"),
        ("o", "json"),
        ("q", query),
        ("s", "0"),
        ("v", "1"),
    ];
    let query_string: String = pairs
        .iter()
        .map(|(key, value)| format!("{}={}", form_encode(key), form_encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("https://{SEARCH_HOST}/html?{query_string}")
}

/// `application/x-www-form-urlencoded` percent-encoding: space as `+`,
/// unreserved characters preserved — the same rule set `sms.rs` already
/// hand-rolled for its form body.
fn form_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                if let Some(byte) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    out.push(byte);
                    index += 3;
                    continue;
                }
                out.push(bytes[index]);
                index += 1;
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Decodes DuckDuckGo's redirect-wrapper href
/// (`//duckduckgo.com/l/?uddg=<percent-encoded-url>&rut=...`) into the
/// real target URL, matching Go's `extractLink`. Returns `""` when the
/// `uddg` parameter is absent.
fn extract_link(href: &str) -> String {
    let href = format!("https:{}", href.trim());
    let Some(query) = href.split_once('?').map(|(_, query)| query) else {
        return String::new();
    };
    for pair in query.split('&') {
        if let Some(value) = pair.strip_prefix("uddg=") {
            return percent_decode(value);
        }
    }
    String::new()
}

/// Scrapes DuckDuckGo's HTML result page via CSS selectors, matching
/// Go's `goquery`-based `parse`.
fn parse_results(html: &str) -> Vec<SearchResult> {
    let document = Html::parse_document(html);
    let result_sel = Selector::parse(".result").unwrap();
    let title_sel = Selector::parse(".result__title a").unwrap();
    let url_sel = Selector::parse(".result__url").unwrap();
    let snippet_sel = Selector::parse(".result__snippet").unwrap();

    document
        .select(&result_sel)
        .map(|element| {
            let title = element
                .select(&title_sel)
                .next()
                .map(|node| node.text().collect::<String>())
                .unwrap_or_default();
            let href = element
                .select(&url_sel)
                .next()
                .and_then(|node| node.value().attr("href"))
                .unwrap_or("");
            let snippet = element
                .select(&snippet_sel)
                .next()
                .map(|node| node.text().collect::<String>())
                .unwrap_or_default();
            SearchResult {
                title,
                link: extract_link(href),
                snippet,
            }
        })
        .collect()
}

fn search<H: HttpTransport>(
    transport: &H,
    query: &str,
    user_agent: &str,
    referrer: &str,
) -> Result<Vec<SearchResult>, String> {
    let request = HttpRequest {
        method: "GET".to_owned(),
        url: build_search_url(query),
        headers: vec![
            ("Referrer".to_owned(), referrer.to_owned()),
            ("User-Agent".to_owned(), user_agent.to_owned()),
            ("Cookie".to_owned(), "kl=wt-wt".to_owned()),
            (
                "Content-Type".to_owned(),
                "x-www-form-urlencoded".to_owned(),
            ),
        ],
        body: None,
    };
    let response = transport
        .send(&request)
        .map_err(|error| format!("failed to send request: {error}"))?;
    Ok(parse_results(&String::from_utf8_lossy(&response.body)))
}

fn results_to_json(results: &[SearchResult]) -> String {
    let array: Vec<Value> = results
        .iter()
        .map(|result| {
            let mut map = Map::new();
            map.insert("title".to_owned(), Value::String(result.title.clone()));
            map.insert("link".to_owned(), Value::String(result.link.clone()));
            map.insert("snippet".to_owned(), Value::String(result.snippet.clone()));
            Value::Object(map)
        })
        .collect();
    serde_json::to_string(&Value::Array(array)).unwrap_or_default()
}

fn is_help(arg: &str) -> bool {
    matches!(arg, "-h" | "-?" | "--help")
}

fn is_version(arg: &str) -> bool {
    matches!(arg, "-v" | "--version")
}

fn parse_args(args: &[OsString]) -> Result<Action, String> {
    let text_args: Vec<String> = args
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();

    let mut parsed = ParsedArgs::default();
    let mut index = 0;
    while index < text_args.len() {
        let arg = text_args[index].as_str();
        if is_help(arg) {
            return Ok(Action::ShowHelp);
        }
        if is_version(arg) {
            return Ok(Action::ShowVersion);
        }
        match arg {
            "-j" | "--json" => parsed.json = true,
            "-t" | "--timeout" => {
                let value = next_value(&text_args, &mut index, arg)?;
                parsed.timeout = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| format!("{arg}: invalid timeout {value:?}"))?,
                );
            }
            "-u" | "--user-agent" => {
                parsed.user_agent = Some(next_value(&text_args, &mut index, arg)?);
            }
            "-r" | "--referrer" => {
                parsed.referrer = Some(next_value(&text_args, &mut index, arg)?);
            }
            "-b" | "--browser" => {
                parsed.browser = Some(next_value(&text_args, &mut index, arg)?);
            }
            "--open" => {
                let value = next_value(&text_args, &mut index, arg)?;
                let n = value
                    .parse::<usize>()
                    .map_err(|_| format!("--open: invalid index {value:?}"))?;
                if n == 0 {
                    return Err("--open: index must be 1 or greater".to_owned());
                }
                parsed.open = Some(n);
            }
            other if other.starts_with('-') && other.len() > 1 => {
                return Err(format!("unknown flag {other:?}"));
            }
            other => parsed.query_words.push(other.to_owned()),
        }
        index += 1;
    }
    Ok(Action::Run(parsed))
}

fn next_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn usage_text(version: &str) -> String {
    format!(
        "{PROGRAM_NAME} v{version}\n\
DuckDuckGo search utility\n\
\n\
Usage\n\
  {PROGRAM_NAME} [options] [query]\n\
\n\
  With no -j/--open, opens an interactive fuzzy-finder result picker when\n\
  stdout is a terminal (Enter opens the selected result, Esc cancels).\n\
  Falls back to a numbered list when stdout is not a terminal.\n\
\n\
Options\n\
  -j, --json         Output results in JSON format\n\
  -t, --timeout      Timeout in seconds (default: 5)\n\
  -u, --user-agent   Custom User-Agent header\n\
  -r, --referrer     Custom Referrer header\n\
  -b, --browser      Browser command to open URLs\n\
  --open N           Open the Nth result (1-based) instead of the picker\n\
  -v, --version      Show version and exit\n\
  -h, -?, --help     Show this help message and exit\n\
\n\
Examples\n\
  {PROGRAM_NAME} golang\n\
  {PROGRAM_NAME} -j golang\n\
  {PROGRAM_NAME} --open 1 golang\n\
  {PROGRAM_NAME} -t 10 -b firefox golang\n"
    )
}

/// Runs `web` and writes its process output to the supplied streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let parsed = match parse_args(&args) {
        Ok(Action::ShowHelp) => {
            let _ = write!(stdout, "{}", usage_text(version));
            return 0;
        }
        Ok(Action::ShowVersion) => {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            return 0;
        }
        Ok(Action::Run(parsed)) => parsed,
        Err(message) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {message}");
            return 1;
        }
    };

    let query = parsed.query_words.join(" ");
    let query = query.trim();
    if query.is_empty() {
        let _ = writeln!(stderr, "{PROGRAM_NAME}: search query is empty");
        return 1;
    }

    let timeout_secs = parsed
        .timeout
        .or_else(|| env::var("DUCKGO_TIMEOUT").ok().and_then(|v| v.parse().ok()))
        .unwrap_or(DEFAULT_TIMEOUT_SECS);
    let user_agent = parsed
        .user_agent
        .or_else(|| env::var("DUCKGO_USER_AGENT").ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| DEFAULT_USER_AGENT.to_owned());
    let referrer = parsed
        .referrer
        .or_else(|| env::var("DUCKGO_REFERRER").ok().filter(|v| !v.is_empty()))
        .unwrap_or_else(|| DEFAULT_REFERRER.to_owned());

    let transport = TimeoutHttpTransport {
        timeout: Duration::from_secs(timeout_secs),
    };

    let results = match search(&transport, query, &user_agent, &referrer) {
        Ok(results) => results,
        Err(message) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {message}");
            return 1;
        }
    };

    let interactive = io::stdout().is_terminal() && io::stdin().is_terminal();

    finish(
        &results,
        parsed.json,
        parsed.open,
        parsed.browser.as_deref(),
        interactive,
        &InteractivePicker,
        stdout,
        stderr,
    )
}

/// Opens `result` via the browser opener the `-b`/`--browser` flag selects
/// (or the system default), matching Go's `open.Run`/`open.RunWith` split.
fn open_link(result: &SearchResult, browser: Option<&str>) -> io::Result<()> {
    match browser {
        Some(command) => CommandOpener {
            command: command.to_owned(),
        }
        .open(Path::new(&result.link)),
        None => crate::mdview::SystemOpener.open(Path::new(&result.link)),
    }
}

#[allow(clippy::too_many_arguments)]
fn finish<P: ResultPicker, W: Write, E: Write>(
    results: &[SearchResult],
    json: bool,
    open: Option<usize>,
    browser: Option<&str>,
    interactive: bool,
    picker: &P,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    if json {
        let _ = writeln!(stdout, "{}", results_to_json(results));
        return 0;
    }

    if let Some(index) = open {
        let Some(result) = results.get(index - 1) else {
            let _ = writeln!(
                stderr,
                "{PROGRAM_NAME}: no result at index {index} ({} result(s) found)",
                results.len()
            );
            return 1;
        };
        return match open_link(result, browser) {
            Ok(()) => {
                let _ = writeln!(
                    stdout,
                    "Opening [{index}] {}: {}",
                    result.title, result.link
                );
                0
            }
            Err(error) => {
                let _ = writeln!(stderr, "{PROGRAM_NAME}: opening browser: {error}");
                1
            }
        };
    }

    if interactive {
        return match picker.pick(results) {
            // Matching Go's picker path exactly: open silently, no
            // confirmation message (unlike the `--open N` path above,
            // which is this port's own addition, not Go behavior).
            Ok(Some(result)) => match open_link(&result, browser) {
                Ok(()) => 0,
                Err(error) => {
                    let _ = writeln!(stderr, "{PROGRAM_NAME}: opening browser: {error}");
                    1
                }
            },
            // Cancelled without selecting: silent success, matching Go's
            // empty-selection no-op.
            Ok(None) => 0,
            Err(error) => {
                let _ = writeln!(stderr, "{PROGRAM_NAME}: picker: {error}");
                1
            }
        };
    }

    for (position, result) in results.iter().enumerate() {
        let _ = writeln!(
            stdout,
            "{}. {}\n   {}",
            position + 1,
            result.title,
            result.link
        );
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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

    fn ok_transport(body: &str) -> FakeTransport {
        FakeTransport {
            response: Mutex::new(Some(Ok(HttpResponse {
                status: 200,
                body: body.as_bytes().to_vec(),
            }))),
            captured: Mutex::new(None),
        }
    }

    #[test]
    fn build_search_url_matches_go_exactly() {
        assert_eq!(
            build_search_url("golang"),
            "https://html.duckduckgo.com/html?api=%2Fd.js&dc=1&o=json&q=golang&s=0&v=1"
        );
    }

    #[test]
    fn extract_link_decodes_uddg_and_handles_missing_param() {
        assert_eq!(
            extract_link("//duckduckgo.com/l/?uddg=https%3A%2F%2Fgo.dev%2F&rut=x"),
            "https://go.dev/"
        );
        assert_eq!(extract_link("//duckduckgo.com/l/?rut=something"), "");
    }

    #[test]
    fn parse_results_scrapes_title_link_and_snippet() {
        let html = r#"
<html><body>
  <div class="result">
    <div class="result__title"><a>Golang Official</a></div>
    <a class="result__url" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fgo.dev%2F&amp;rut=x"></a>
    <div class="result__snippet">Go <b>language</b> site</div>
  </div>
</body></html>"#;
        let results = parse_results(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Golang Official");
        assert_eq!(results[0].link, "https://go.dev/");
        assert_eq!(results[0].snippet.trim(), "Go language site");
    }

    #[test]
    fn search_sends_referrer_and_user_agent_headers() {
        let transport = ok_transport("<html><body></body></html>");
        let result = search(&transport, "golang", "ua-test", "https://ref.example");
        assert!(result.is_ok());
        let captured = transport.captured.lock().unwrap().clone().unwrap();
        assert!(
            captured
                .headers
                .contains(&("Referrer".to_owned(), "https://ref.example".to_owned()))
        );
        assert!(
            captured
                .headers
                .contains(&("User-Agent".to_owned(), "ua-test".to_owned()))
        );
    }

    #[test]
    fn empty_query_is_rejected_before_any_request() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run(["   "], "1.0.0", &mut stdout, &mut stderr);
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("search query is empty")
        );
    }

    #[test]
    fn json_mode_prints_result_array_and_skips_open() {
        let results = vec![SearchResult {
            title: "Golang Official".to_owned(),
            link: "https://go.dev/".to_owned(),
            snippet: "Go language site".to_owned(),
        }];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = finish(
            &results,
            true,
            None,
            None,
            false,
            &UnusedPicker,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.contains("\"title\":\"Golang Official\""));
        assert!(text.contains("\"link\":\"https://go.dev/\""));
        assert!(stderr.is_empty());
    }

    #[test]
    fn default_mode_prints_numbered_list() {
        let results = vec![
            SearchResult {
                title: "First".to_owned(),
                link: "https://first.example".to_owned(),
                snippet: String::new(),
            },
            SearchResult {
                title: "Second".to_owned(),
                link: "https://second.example".to_owned(),
                snippet: String::new(),
            },
        ];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = finish(
            &results,
            false,
            None,
            None,
            false,
            &UnusedPicker,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.contains("1. First"));
        assert!(text.contains("2. Second"));
        assert!(stderr.is_empty());
    }

    #[test]
    fn open_out_of_range_reports_error_without_opening() {
        let results = vec![SearchResult {
            title: "Only".to_owned(),
            link: "https://only.example".to_owned(),
            snippet: String::new(),
        }];
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = finish(
            &results,
            false,
            Some(2),
            None,
            false,
            &UnusedPicker,
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 1);
        assert!(stdout.is_empty());
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("no result at index 2")
        );
    }

    #[test]
    fn open_uses_custom_browser_command() {
        let directory = std::env::temp_dir().join(format!("rkit-web-unit-{}", std::process::id()));
        let _ = std::fs::create_dir(&directory);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let marker = directory.join("opened.txt");
            let script = directory.join("fake-browser.sh");
            std::fs::write(
                &script,
                format!("#!/bin/sh\necho \"$1\" > {}\n", marker.display()),
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

            let results = vec![SearchResult {
                title: "Golang".to_owned(),
                link: "https://go.dev/".to_owned(),
                snippet: String::new(),
            }];
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = finish(
                &results,
                false,
                Some(1),
                Some(script.to_str().unwrap()),
                false,
                &UnusedPicker,
                &mut stdout,
                &mut stderr,
            );
            assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
            assert_eq!(fs_read(&marker), "https://go.dev/\n");
        }
        let _ = std::fs::remove_dir_all(directory);
    }

    #[cfg(unix)]
    fn fs_read(path: &std::path::Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    struct UnusedPicker;
    impl ResultPicker for UnusedPicker {
        fn pick(&self, _results: &[SearchResult]) -> io::Result<Option<SearchResult>> {
            panic!(
                "picker should not be invoked: json/--open handle the request first, or interactive is false"
            );
        }
    }

    struct FakePicker(Option<usize>);
    impl ResultPicker for FakePicker {
        fn pick(&self, results: &[SearchResult]) -> io::Result<Option<SearchResult>> {
            Ok(self.0.map(|index| results[index].clone()))
        }
    }

    fn sample_results() -> Vec<SearchResult> {
        vec![
            SearchResult {
                title: "First".to_owned(),
                link: "https://first.example".to_owned(),
                snippet: "the first snippet".to_owned(),
            },
            SearchResult {
                title: "Second".to_owned(),
                link: "https://second.example".to_owned(),
                snippet: "the second snippet".to_owned(),
            },
        ]
    }

    #[test]
    fn interactive_picker_selection_opens_silently_without_confirmation() {
        let directory =
            std::env::temp_dir().join(format!("rkit-web-picker-unit-{}", std::process::id()));
        let _ = std::fs::create_dir(&directory);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let marker = directory.join("opened.txt");
            let script = directory.join("fake-browser.sh");
            std::fs::write(
                &script,
                format!("#!/bin/sh\necho \"$1\" > {}\n", marker.display()),
            )
            .unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

            let results = sample_results();
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            let code = finish(
                &results,
                false,
                None,
                Some(script.to_str().unwrap()),
                true,
                &FakePicker(Some(1)),
                &mut stdout,
                &mut stderr,
            );
            assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
            assert!(
                stdout.is_empty(),
                "picker path prints no confirmation, matching Go"
            );
            assert_eq!(fs_read(&marker), "https://second.example\n");
        }
        let _ = std::fs::remove_dir_all(directory);
    }

    #[test]
    fn interactive_picker_cancel_is_silent_success() {
        let results = sample_results();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = finish(
            &results,
            false,
            None,
            None,
            true,
            &FakePicker(None),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
    }

    #[test]
    fn result_renderer_truncates_long_snippets_onto_one_line() {
        let long_snippet = "word ".repeat(40);
        let result = SearchResult {
            title: "Title".to_owned(),
            link: "https://example.com".to_owned(),
            snippet: long_snippet,
        };
        let rendered = ResultRenderer.render(&result);
        assert!(!rendered.contains('\n'));
        assert!(rendered.starts_with("Title  "));
        assert!(rendered.chars().count() <= "Title  ".len() + SNIPPET_DISPLAY_CHARS + 1);
        assert!(rendered.ends_with('…'));
    }

    #[test]
    fn truncate_snippet_collapses_whitespace_and_marks_truncation() {
        assert_eq!(truncate_snippet("a  b\nc", 80), "a b c");
        let long = "x".repeat(100);
        let truncated = truncate_snippet(&long, 10);
        assert_eq!(truncated.chars().count(), 11);
        assert!(truncated.ends_with('…'));
    }
}
