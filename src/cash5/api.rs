//! Live draw fetching for `cash5`: the primary NJ Lottery JSON API and the
//! `lottonumbers.com` HTML-scraper backup, ported from Go's `api.go`.
//!
//! Both the connectivity check and the HTTP transport are injectable, so
//! no test in this file contacts a real network host (`## Out Of Scope`).

use crate::cash5::dates::{self, CivilDate};
use crate::cash5::model::{self, Draw, DrawResult};
use crate::color::{ColorMode, RED3};
use crate::pman::{HttpRequest, HttpResponse, HttpTransport};
use openssl::ssl::{SslConnector, SslConnectorBuilder, SslMethod, SslVerifyMode};
use openssl::x509::X509;
use scraper::{CaseSensitivity, ElementRef, Html, Selector};
use std::collections::HashSet;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::Command;
use std::time::Duration;

const BASE_URL: &str = "https://www.njlottery.com/api/v1/draw-games/draws/page";
const LOTTONUMBERS_URL: &str = "https://new-jersey.lottonumbers.com/cash-5/results";

/// Reports whether a network connection can currently be established;
/// injectable so no test performs a real network dial.
pub trait ConnectivityCheck {
    fn is_online(&self) -> bool;
}

/// Dials Cloudflare DNS over TCP — fast, reliable, no HTTP overhead.
pub struct TcpConnectivityCheck;

impl ConnectivityCheck for TcpConnectivityCheck {
    fn is_online(&self) -> bool {
        let addr: SocketAddr = "1.1.1.1:53".parse().expect("valid literal address");
        TcpStream::connect_timeout(&addr, Duration::from_secs(3)).is_ok()
    }
}

/// A source that can return NJ Cash 5 draws.
pub trait DrawFetcher {
    fn name(&self) -> &'static str;
    fn fetch_recent(&self, date_from_millis: i64, date_to_millis: i64)
    -> Result<Vec<Draw>, String>;
}

/// Scrapes `new-jersey.lottonumbers.com`'s results page (server-rendered
/// HTML, no API). `lotterypost.com` is blocked by Cloudflare and
/// `lotteryusa.com` returns 403 — matching Go's documented fetcher choice.
pub struct LottoNumbersFetcher<'a, H: HttpTransport> {
    pub transport: &'a H,
}

impl<H: HttpTransport> DrawFetcher for LottoNumbersFetcher<'_, H> {
    fn name(&self) -> &'static str {
        "new-jersey.lottonumbers.com"
    }

    fn fetch_recent(
        &self,
        date_from_millis: i64,
        date_to_millis: i64,
    ) -> Result<Vec<Draw>, String> {
        let request = HttpRequest {
            method: "GET".to_owned(),
            url: LOTTONUMBERS_URL.to_owned(),
            headers: vec![
                (
                    "User-Agent".to_owned(),
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36".to_owned(),
                ),
                (
                    "Accept".to_owned(),
                    "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8".to_owned(),
                ),
            ],
            body: None,
        };
        let response = self
            .transport
            .send(&request)
            .map_err(|error| format!("{}: request failed: {error}", self.name()))?;
        if response.status != 200 {
            return Err(format!("{}: bad status {}", self.name(), response.status));
        }
        let html = String::from_utf8_lossy(&response.body);
        let draws =
            parse_lottonumbers_draws(&html).map_err(|error| format!("{}: {error}", self.name()))?;
        Ok(draws
            .into_iter()
            .filter(|draw| draw.draw_time >= date_from_millis && draw.draw_time <= date_to_millis)
            .collect())
    }
}

const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn parse_month_day_comma_year(text: &str) -> Option<CivilDate> {
    let text = text.trim();
    let (month_word, rest) = text.split_once(' ')?;
    let month = MONTH_NAMES
        .iter()
        .position(|name| name.eq_ignore_ascii_case(month_word))? as i64
        + 1;
    let (day_text, year_text) = rest.split_once(',')?;
    let day: i64 = day_text.trim().parse().ok()?;
    let year: i64 = year_text.trim().parse().ok()?;
    Some(CivilDate { year, month, day })
}

/// Extracts draw data from the lottonumbers.com HTML tree.
///
/// Target structure (repeated per draw):
/// ```html
/// <div class="draw">
///   <div class="resultBox">
///     <div><strong>Saturday</strong><br>February 21, 2026</div>
///     <ul class="balls multiplier">
///       <li class="ball ball">14</li>       <!-- regular numbers (4 of these) -->
///       <li class="ball bullseye">20</li>   <!-- 5th number / bullseye -->
///       <li class="ball xtra-number">3</li> <!-- Xtra multiplier, skip -->
///     </ul>
///   </div>
///   <div class="resultBoxStats"><p>Jackpot: <strong>$764,968</strong></p></div>
/// </div>
/// ```
fn parse_lottonumbers_draws(html: &str) -> Result<Vec<Draw>, String> {
    let document = Html::parse_document(html);
    let draw_sel = Selector::parse(".draw").unwrap();
    let result_box_sel = Selector::parse(".resultBox").unwrap();
    let result_box_stats_sel = Selector::parse(".resultBoxStats").unwrap();
    let ul_sel = Selector::parse("ul").unwrap();
    let strong_sel = Selector::parse("strong").unwrap();
    let div_sel = Selector::parse("div").unwrap();

    let mut draws = Vec::new();
    for draw_el in document.select(&draw_sel) {
        if let Some(draw) = parse_single_lottonumbers_draw(
            draw_el,
            &result_box_sel,
            &result_box_stats_sel,
            &ul_sel,
            &strong_sel,
            &div_sel,
        ) {
            draws.push(draw);
        }
    }
    if draws.is_empty() {
        return Err("no draws found in page — site structure may have changed".to_owned());
    }
    Ok(draws)
}

fn parse_single_lottonumbers_draw(
    draw_el: ElementRef,
    result_box_sel: &Selector,
    result_box_stats_sel: &Selector,
    ul_sel: &Selector,
    strong_sel: &Selector,
    div_sel: &Selector,
) -> Option<Draw> {
    let result_box = draw_el.select(result_box_sel).next()?;
    let result_box_stats = draw_el.select(result_box_stats_sel).next();

    let first_div = result_box.select(div_sel).next()?;
    let date_text = first_div
        .children()
        .filter_map(|node| node.value().as_text())
        .map(|text| text.trim())
        .rfind(|text| !text.is_empty())?;
    let date = parse_month_day_comma_year(date_text)?;
    // Draw time is 10:57 PM ET.
    let millis = dates::millis_from_eastern_civil(date, 22, 57, 0);
    let id = format!("lottonumbers-{}", dates::ymd(date));

    let ul = result_box.select(ul_sel).next()?;
    let mut numbers: Vec<String> = Vec::new();
    for li in ul.children().filter_map(ElementRef::wrap) {
        if li.value().name() != "li" {
            continue;
        }
        if li
            .value()
            .has_class("xtra-number", CaseSensitivity::CaseSensitive)
        {
            continue;
        }
        let text: String = li.text().collect();
        let text = text.trim();
        if !text.is_empty() {
            numbers.push(text.to_owned());
        }
    }
    if numbers.len() < 5 {
        return None;
    }
    numbers.truncate(5);

    let mut estimated_jackpot = 0i64;
    if let Some(stats) = result_box_stats {
        for strong in stats.select(strong_sel) {
            let text: String = strong.text().collect();
            let text = text.trim();
            if let Some(rest) = text.strip_prefix('$') {
                let cleaned = rest.replace(',', "");
                if let Ok(dollars) = cleaned.parse::<i64>() {
                    estimated_jackpot = dollars * 100;
                }
            }
        }
    }

    Some(Draw {
        game_name: "Cash 5".to_owned(),
        id,
        draw_time: millis,
        estimated_jackpot,
        results: vec![DrawResult {
            primary: numbers,
            ..Default::default()
        }],
        ..Default::default()
    })
}

/// Minimal HTTPS/1.1 client with a fixed 15-30s timeout, driving the
/// shared `pman` request/response types. Duplicated from `web.rs`'s
/// equivalent transport (out of this AC's file scope, and not `pub`)
/// rather than shared.
pub struct TimeoutHttpTransport {
    pub timeout: Duration,
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
        let addr = std::net::ToSocketAddrs::to_socket_addrs(&address)?
            .next()
            .ok_or_else(|| io::Error::other(format!("could not resolve {host}")))?;
        let stream = TcpStream::connect_timeout(&addr, self.timeout)?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;

        let mut builder = SslConnector::builder(SslMethod::tls())
            .map_err(|error| io::Error::other(error.to_string()))?;
        builder.set_verify(SslVerifyMode::PEER);
        configure_trust(&mut builder).map_err(io::Error::other)?;
        let connector = builder.build();
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

/// Loads trusted root certificates: vendored OpenSSL default paths first,
/// falling back to macOS's System Root keychain — vendored/statically-
/// linked OpenSSL doesn't automatically see the macOS Keychain otherwise.
/// Adapted from `certls.rs`/`web.rs` (duplicated rather than reused, since
/// neither is in this AC's file scope nor exposes these as `pub`).
fn configure_trust(builder: &mut SslConnectorBuilder) -> Result<(), String> {
    let default_error = builder
        .set_default_verify_paths()
        .err()
        .map(|error| error.to_string());

    #[cfg(target_os = "macos")]
    {
        let keychain_error = load_macos_keychain_roots(builder).err();
        if default_error.is_some() && keychain_error.is_some() {
            return Err(format!(
                "default paths: {}; macOS keychain: {}",
                default_error.unwrap_or_default(),
                keychain_error.unwrap_or_default()
            ));
        }
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    default_error.map_or(Ok(()), Err)
}

#[cfg(target_os = "macos")]
fn load_macos_keychain_roots(builder: &mut SslConnectorBuilder) -> Result<usize, String> {
    let keychains = [
        "/System/Library/Keychains/SystemRootCertificates.keychain",
        "/Library/Keychains/System.keychain",
    ];
    let mut loaded = 0;
    let mut last_error = None;
    for path in keychains {
        let output = match Command::new("/usr/bin/security")
            .args(["find-certificate", "-a", "-p", path])
            .output()
        {
            Ok(output) => output,
            Err(error) => {
                last_error = Some(error.to_string());
                continue;
            }
        };
        if !output.status.success() {
            last_error = Some(String::from_utf8_lossy(&output.stderr).trim().to_string());
            continue;
        }
        match add_pem_certificates(builder, &output.stdout) {
            Ok(count) => loaded += count,
            Err(error) => last_error = Some(error),
        }
    }
    if loaded == 0 {
        return Err(last_error.unwrap_or_else(|| "no macOS keychain certificates found".into()));
    }
    Ok(loaded)
}

fn add_pem_certificates(builder: &mut SslConnectorBuilder, pem: &[u8]) -> Result<usize, String> {
    let certificates = X509::stack_from_pem(pem).map_err(|error| error.to_string())?;
    let mut loaded = 0;
    for certificate in certificates {
        if builder.cert_store_mut().add_cert(certificate).is_ok() {
            loaded += 1;
        }
    }
    Ok(loaded)
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

/// Fetches one page of draws from the primary API.
pub fn fetch_page_with_size<H: HttpTransport>(
    transport: &H,
    page: i64,
    size: i64,
    date_from: i64,
    date_to: i64,
) -> Result<Vec<Draw>, String> {
    let url = format!(
        "{BASE_URL}?game-names=Cash+5&status=CLOSED&size={size}&page={page}&date-from={date_from}&date-to={date_to}"
    );
    let request = HttpRequest {
        method: "GET".to_owned(),
        url,
        headers: vec![
            ("Accept".to_owned(), "application/json".to_owned()),
            (
                "Referer".to_owned(),
                "https://www.njlottery.com/en-us/drawgames/jerseycash.html".to_owned(),
            ),
            ("User-Agent".to_owned(), "Mozilla/5.0".to_owned()),
        ],
        body: None,
    };
    let response = transport
        .send(&request)
        .map_err(|error| error.to_string())?;
    if response.status == 404 {
        return Err(format!("primary source returned 404: {}", response.status));
    }
    if response.status != 200 {
        return Err(format!("bad status: {}", response.status));
    }
    model::parse_api_response(&response.body)
}

pub fn is_404_error(message: &str) -> bool {
    message.contains("404")
}

/// Pages through the API for a date-from/date-to range, saving after each
/// page via `save_callback`. Stops at ~365 draws or when data runs out. A
/// non-500 error fails immediately; a 500 persisting through 5 retries
/// (2s apart) falls back to returning whatever was already fetched.
pub fn fetch_draws_by_date_range<H, F, E>(
    transport: &H,
    from_millis: i64,
    to_millis: i64,
    existing: &[Draw],
    mut save_callback: F,
    stdout: &mut E,
) -> Result<Vec<Draw>, String>
where
    H: HttpTransport,
    F: FnMut(&[Draw]) -> io::Result<()>,
    E: Write,
{
    const PAGE_SIZE: i64 = 365;
    const MAX_DRAWS: i64 = 365;
    let mut all: Vec<Draw> = existing.to_vec();
    let mut page = 0i64;
    let mut new_draws_count = 0i64;

    loop {
        let mut page_draws: Vec<Draw> = Vec::new();
        let mut retry_error: Option<String> = None;
        for attempt in 1..=5 {
            match fetch_page_with_size(transport, page, PAGE_SIZE, from_millis, to_millis) {
                Ok(draws) => {
                    page_draws = draws;
                    retry_error = None;
                    break;
                }
                Err(message) if message.contains("500") => {
                    let _ = writeln!(stdout, "Server 500 on page {page}, retry {attempt}/5...");
                    std::thread::sleep(Duration::from_secs(2));
                    retry_error = Some(message);
                }
                Err(message) => return Err(message),
            }
        }
        if let Some(error) = retry_error {
            if new_draws_count > 0 {
                let _ = writeln!(
                    stdout,
                    "Error fetching page {page} after retries. Saved {new_draws_count} draws successfully."
                );
                return Ok(all);
            }
            return Err(format!("page {page} failed after retries: {error}"));
        }

        if page_draws.is_empty() {
            break;
        }

        let got = page_draws.len() as i64;
        all.extend(page_draws);
        new_draws_count += got;
        all.sort_by_key(|draw| draw.draw_time);

        if let Err(error) = save_callback(&all) {
            let _ = writeln!(stdout, "Warning: failed to save after page {page}: {error}");
        }

        if got < PAGE_SIZE {
            break;
        }
        if page == 0 && got >= 350 {
            break;
        }
        if new_draws_count >= MAX_DRAWS {
            let _ = writeln!(stdout, "Reached ~365 draws limit, stopping fetch");
            break;
        }
        page += 1;
    }
    Ok(all)
}

/// Fetches draws in year-long chunks: the last year on a cold cache, or
/// the year before the oldest cached draw otherwise. Prints a progress
/// summary line unconditionally (even on error), matching Go.
pub fn fetch_all_draws_incremental<H, F, E>(
    transport: &H,
    existing: Vec<Draw>,
    now_millis: i64,
    mut save_callback: F,
    stdout: &mut E,
) -> Result<Vec<Draw>, String>
where
    H: HttpTransport,
    F: FnMut(&[Draw]) -> io::Result<()>,
    E: Write,
{
    const YEAR_MILLIS: i64 = 365 * 86_400_000;
    let (date_from, date_to) = if existing.is_empty() {
        (now_millis - YEAR_MILLIS, now_millis)
    } else {
        let oldest = existing
            .iter()
            .map(|d| d.draw_time)
            .min()
            .unwrap_or(now_millis);
        let date_to = oldest - 1;
        (date_to - YEAR_MILLIS, date_to)
    };

    let before_count = existing.len();
    let outcome = fetch_draws_by_date_range(
        transport,
        date_from,
        date_to,
        &existing,
        &mut save_callback,
        stdout,
    );
    let all_len = outcome.as_ref().map(Vec::len).unwrap_or(0);
    let new_count = all_len as i64 - before_count as i64;
    let period_str = format!(
        "{} → {}",
        dates::ymd(dates::civil_time_at_offset(date_from, 0).date),
        dates::ymd(dates::civil_time_at_offset(date_to, 0).date)
    );
    let _ = writeln!(stdout, "{period_str:<33}  {new_count:>7}  {all_len:>12}");
    outcome
}

/// Fetches the most recent draw (any status) to read the current jackpot.
pub fn fetch_current_jackpot<H: HttpTransport>(transport: &H) -> Result<i64, String> {
    let url = format!("{BASE_URL}?game-names=Cash+5&size=1&page=0");
    let request = HttpRequest {
        method: "GET".to_owned(),
        url,
        headers: vec![
            ("Accept".to_owned(), "application/json".to_owned()),
            (
                "Referer".to_owned(),
                "https://www.njlottery.com/en-us/drawgames/jerseycash.html".to_owned(),
            ),
            ("User-Agent".to_owned(), "Mozilla/5.0".to_owned()),
        ],
        body: None,
    };
    let response = transport
        .send(&request)
        .map_err(|error| error.to_string())?;
    let draws = model::parse_api_response(&response.body)?;
    draws
        .first()
        .map(|draw| draw.estimated_jackpot)
        .ok_or_else(|| "no draws found".to_owned())
}

/// Eastern-Time calendar-day key, used to merge backup draws into
/// existing ones by date (primary/backup IDs differ). Using Eastern Time
/// for both sources (rather than Go's operator-local `time.Now()`
/// location) avoids a latent cross-timezone merge mismatch, since both
/// sources' `draw_time` values are themselves NJ-anchored.
fn date_key(millis: i64) -> String {
    dates::ymd(dates::eastern_civil_time(millis).date)
}

/// Attempts each backup source in order, merges any draws found into
/// `existing`, saves, and returns the result. Prints status for each
/// attempt.
pub(crate) fn try_backup_fetchers<F, E>(
    fetchers: &[&dyn DrawFetcher],
    mut existing: Vec<Draw>,
    date_from: i64,
    date_to: i64,
    mut save_callback: F,
    color: ColorMode,
    stdout: &mut E,
) -> Vec<Draw>
where
    F: FnMut(&[Draw]) -> io::Result<()>,
    E: Write,
{
    for fetcher in fetchers {
        let _ = writeln!(stdout, "  Trying {}...", fetcher.name());
        let draws = match fetcher.fetch_recent(date_from, date_to) {
            Ok(draws) => draws,
            Err(error) => {
                let _ = writeln!(stdout, "  {} failed: {error}", fetcher.name());
                continue;
            }
        };
        if draws.is_empty() {
            let _ = writeln!(stdout, "  {}: no new draws found", fetcher.name());
            continue;
        }

        let existing_dates: HashSet<String> =
            existing.iter().map(|d| date_key(d.draw_time)).collect();
        let mut added = 0;
        for draw in draws {
            let key = date_key(draw.draw_time);
            if !existing_dates.contains(&key) {
                existing.push(draw);
                added += 1;
            }
        }
        existing.sort_by_key(|d| d.draw_time);

        if added > 0 {
            let _ = writeln!(
                stdout,
                "  {}: added {added} draw(s) via backup",
                fetcher.name()
            );
            if let Err(error) = save_callback(&existing) {
                let _ = writeln!(
                    stdout,
                    "  Warning: failed to save after backup fetch: {error}"
                );
            }
        } else {
            let _ = writeln!(stdout, "  {}: draws already in cache", fetcher.name());
        }
        return existing;
    }

    let _ = writeln!(
        stdout,
        "{}",
        color.paint(RED3, "All backup sources failed — showing cached data")
    );
    existing
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct FakeTransport {
        response: Mutex<Option<Result<HttpResponse, String>>>,
    }

    impl HttpTransport for FakeTransport {
        fn send(&self, _request: &HttpRequest) -> io::Result<HttpResponse> {
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
        }
    }

    #[test]
    fn fetch_page_with_size_returns_404_error_distinctly() {
        let transport = FakeTransport {
            response: Mutex::new(Some(Ok(HttpResponse {
                status: 404,
                body: Vec::new(),
            }))),
        };
        let error = fetch_page_with_size(&transport, 0, 365, 0, 1).unwrap_err();
        assert!(is_404_error(&error));
    }

    #[test]
    fn fetch_page_with_size_parses_draws_array() {
        let transport = ok_transport(r#"{"draws":[{"id":"d1","gameName":"Cash 5"}]}"#);
        let draws = fetch_page_with_size(&transport, 0, 365, 0, 1).unwrap();
        assert_eq!(draws.len(), 1);
        assert_eq!(draws[0].id, "d1");
    }

    #[test]
    fn fetch_draws_by_date_range_stops_on_short_page() {
        let transport = ok_transport(r#"{"draws":[{"id":"d1","drawTime":1000}]}"#);
        let mut stdout = Vec::new();
        let saved = std::cell::RefCell::new(Vec::new());
        let result = fetch_draws_by_date_range(
            &transport,
            0,
            1,
            &[],
            |draws| {
                *saved.borrow_mut() = draws.to_vec();
                Ok(())
            },
            &mut stdout,
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(saved.borrow().len(), 1);
    }

    #[test]
    fn fetch_draws_by_date_range_fails_immediately_on_non_500_error() {
        let transport = FakeTransport {
            response: Mutex::new(Some(Err("boom".to_owned()))),
        };
        let mut stdout = Vec::new();
        let result = fetch_draws_by_date_range(&transport, 0, 1, &[], |_| Ok(()), &mut stdout);
        assert!(result.is_err());
    }

    #[test]
    fn fetch_current_jackpot_reads_first_draws_estimate() {
        let transport = ok_transport(r#"{"draws":[{"id":"d1","estimatedJackpot":500000}]}"#);
        assert_eq!(fetch_current_jackpot(&transport).unwrap(), 500_000);
    }

    #[test]
    fn parse_lottonumbers_draws_extracts_date_numbers_and_jackpot() {
        let html = r#"
        <html><body>
          <div class="draw">
            <div class="resultBox">
              <div><strong>Saturday</strong><br>February 21, 2026</div>
              <ul class="balls multiplier">
                <li class="ball ball">14</li>
                <li class="ball ball">3</li>
                <li class="ball ball">44</li>
                <li class="ball ball">9</li>
                <li class="ball bullseye">20</li>
                <li class="ball xtra-number">3</li>
              </ul>
            </div>
            <div class="resultBoxStats"><p>Jackpot: <strong>$764,968</strong></p></div>
          </div>
        </body></html>"#;
        let draws = parse_lottonumbers_draws(html).unwrap();
        assert_eq!(draws.len(), 1);
        assert_eq!(
            draws[0].results[0].primary,
            vec!["14", "3", "44", "9", "20"]
        );
        assert_eq!(draws[0].estimated_jackpot, 76_496_800);
        assert_eq!(draws[0].id, "lottonumbers-2026-02-21");
    }

    #[test]
    fn parse_lottonumbers_draws_errors_when_structure_missing() {
        let html = "<html><body><p>nothing here</p></body></html>";
        assert!(parse_lottonumbers_draws(html).is_err());
    }

    #[test]
    fn lottonumbers_fetcher_filters_to_requested_range() {
        let html = r#"
        <div class="draw">
          <div class="resultBox">
            <div><strong>Saturday</strong><br>February 21, 2026</div>
            <ul><li class="ball">1</li><li class="ball">2</li><li class="ball">3</li><li class="ball">4</li><li class="ball">5</li></ul>
          </div>
        </div>"#;
        let transport = ok_transport(html);
        let fetcher = LottoNumbersFetcher {
            transport: &transport,
        };
        let millis = dates::millis_from_eastern_civil(
            CivilDate {
                year: 2026,
                month: 2,
                day: 21,
            },
            22,
            57,
            0,
        );
        let draws = fetcher.fetch_recent(millis + 1, millis + 2).unwrap();
        assert!(draws.is_empty());
    }

    #[test]
    fn try_backup_fetchers_reports_failure_when_all_sources_fail() {
        struct AlwaysFails;
        impl DrawFetcher for AlwaysFails {
            fn name(&self) -> &'static str {
                "always-fails"
            }
            fn fetch_recent(&self, _from: i64, _to: i64) -> Result<Vec<Draw>, String> {
                Err("nope".to_owned())
            }
        }
        let fetchers: Vec<&dyn DrawFetcher> = vec![&AlwaysFails];
        let mut stdout = Vec::new();
        let result = try_backup_fetchers(
            &fetchers,
            vec![],
            0,
            1,
            |_| Ok(()),
            ColorMode::new(false),
            &mut stdout,
        );
        assert!(result.is_empty());
        assert!(
            String::from_utf8(stdout)
                .unwrap()
                .contains("All backup sources failed")
        );
    }
}
