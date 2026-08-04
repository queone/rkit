//! NJ Cash 5 lottery data/statistics/recommendation application, ported
//! from the Go `cash5` command.

pub mod api;
pub mod dates;
pub mod display;
pub mod match_analysis;
pub mod model;
pub mod recommend;
pub mod render;
pub mod stats;
pub mod store;
pub mod strategy;

use api::{ConnectivityCheck, DrawFetcher};
use model::Draw;
use render::TerminalCapability;
use std::collections::HashSet;
use std::ffi::OsString;
use std::io::Write;
use std::time::Duration;

use crate::color::{BLUE7, ColorMode, GRAY5, GREEN3, RED3};
use crate::pman::HttpTransport;

const PROGRAM_NAME: &str = "cash5";
const LOTTERY_WARNING: &str =
    "This is basically lighting money on fire! Play for fun, not profit 😀";

enum Action {
    ShowHelp,
    ShowVersion,
    OddsTable(i64),
    MatchAnalysis(i64),
    Run(ParsedArgs),
}

#[derive(Default)]
struct ParsedArgs {
    fetch_all: bool,
    show_all: bool,
    show_stats: bool,
    debug_date: Option<String>,
}

fn parse_args(args: &[String]) -> Result<Action, String> {
    if args.iter().any(|a| a == "-?" || a == "-h" || a == "--help") {
        return Ok(Action::ShowHelp);
    }
    // -o and -m/--match-analysis are handled before general flag
    // dispatch: cobra can't express an optional-value flag, matching Go's
    // own pre-parse-hack workaround.
    if let Some(index) = args.iter().position(|a| a == "-o") {
        let n = optional_positive_int(args, index).unwrap_or(30);
        return Ok(Action::OddsTable(n));
    }
    if let Some(index) = args
        .iter()
        .position(|a| a == "-m" || a == "--match-analysis")
    {
        let n = optional_positive_int(args, index).unwrap_or(30);
        return Ok(Action::MatchAnalysis(n));
    }

    let mut parsed = ParsedArgs::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "-f" | "--fetch-all" => parsed.fetch_all = true,
            "-a" | "--all" => parsed.show_all = true,
            "-s" | "--stats" => parsed.show_stats = true,
            "-v" | "--version" => return Ok(Action::ShowVersion),
            "-d" | "--debug" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| format!("{arg} requires a value"))?;
                parsed.debug_date = Some(value.clone());
            }
            other => return Err(format!("unknown flag {other:?}")),
        }
        index += 1;
    }
    Ok(Action::Run(parsed))
}

fn optional_positive_int(args: &[String], flag_index: usize) -> Option<i64> {
    args.get(flag_index + 1)
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
}

fn usage_text(version: &str) -> String {
    format!(
        "{PROGRAM_NAME} v{version}\n\
NJ Cash 5 daily numbers recommender\n\
\n\
Usage\n\
  {PROGRAM_NAME} [options]\n\
\n\
Options\n\
  -f              Fetch new draws since last run (within last year)\n\
  -a              Display all previous drawings\n\
  -s              Show statistics about historical data\n\
  -m [N]          Show closest-match analysis for last N drawings (default: 30)\n\
  -o [N]          Show odds table for 1 to N combos played (default: 30)\n\
  -d DATE         Show raw JSON for draws on DATE (format: 2026-02-06)\n\
  -v, --version   Show version and exit\n\
  -h, -?, --help  Show this help message and exit\n\
\n\
Running without switches will\n\
  1. Display the last 10 draws\n\
  2. Show current jackpot, last winning numbers, and closest matches\n\
  3. Recommend 5 sets of numbers based on statistics\n\
\n\
Examples\n\
  {PROGRAM_NAME}\n\
  {PROGRAM_NAME} -f\n\
  {PROGRAM_NAME} -s\n\
  {PROGRAM_NAME} -m 50\n\
  {PROGRAM_NAME} -o 100\n\
  {PROGRAM_NAME} -o\n\
\n\
{LOTTERY_WARNING}\n"
    )
}

/// Runs `cash5` and writes its process output to the supplied streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let paths = match store::Paths::from_env() {
        Ok(paths) => paths,
        Err(error) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
            return 1;
        }
    };
    let transport = api::TimeoutHttpTransport {
        timeout: Duration::from_secs(20),
    };
    run_with(
        args,
        version,
        &paths,
        &transport,
        &api::TcpConnectivityCheck,
        &render::RealTerminal,
        stdout,
        stderr,
    )
}

/// Runs `cash5` against injectable paths, HTTP transport, connectivity
/// check, and terminal-graphics capability, independent of the real
/// filesystem locations, the network, and the real terminal.
#[allow(clippy::too_many_arguments)]
pub fn run_with<I, S, H, C, T, W, E>(
    args: I,
    version: &str,
    paths: &store::Paths,
    transport: &H,
    connectivity: &C,
    terminal: &T,
    stdout: &mut W,
    stderr: &mut E,
) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    H: HttpTransport,
    C: ConnectivityCheck,
    T: TerminalCapability,
    W: Write,
    E: Write,
{
    let args: Vec<String> = args
        .into_iter()
        .map(|arg| arg.into().to_string_lossy().into_owned())
        .collect();

    let action = match parse_args(&args) {
        Ok(action) => action,
        Err(message) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {message}");
            return 1;
        }
    };

    match action {
        Action::ShowHelp => {
            let _ = write!(stdout, "{}", usage_text(version));
            0
        }
        Action::ShowVersion => {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            0
        }
        Action::OddsTable(n) => {
            let jackpot = resolve_jackpot_dollars(transport, paths, stderr);
            let _ = display::display_odds_table(n, jackpot, stdout);
            0
        }
        Action::MatchAnalysis(n) => {
            let draws = match store::load_draws(paths, stderr) {
                Ok(draws) => draws,
                Err(error) => {
                    let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
                    return 1;
                }
            };
            let _ = match_analysis::display_match_analysis(
                &draws,
                n as usize,
                ColorMode::detect_stdout(),
                terminal,
                stdout,
            );
            0
        }
        Action::Run(parsed) => run_action(
            &parsed,
            paths,
            transport,
            connectivity,
            terminal,
            stdout,
            stderr,
        ),
    }
}

fn resolve_jackpot_dollars<H: HttpTransport, E: Write>(
    transport: &H,
    paths: &store::Paths,
    stderr: &mut E,
) -> Option<i64> {
    if let Ok(jackpot) = api::fetch_current_jackpot(transport)
        && jackpot > 0
    {
        return Some(jackpot / 100);
    }
    let draws = store::load_draws(paths, stderr).ok()?;
    let latest = draws.iter().max_by_key(|d| d.draw_time)?;
    (latest.estimated_jackpot > 0).then_some(latest.estimated_jackpot / 100)
}

fn run_action<H: HttpTransport, C: ConnectivityCheck, T: TerminalCapability, W: Write, E: Write>(
    parsed: &ParsedArgs,
    paths: &store::Paths,
    transport: &H,
    connectivity: &C,
    terminal: &T,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    if let Some(date) = &parsed.debug_date {
        let draws = match store::load_draws(paths, stderr) {
            Ok(draws) => draws,
            Err(error) => {
                let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
                return 1;
            }
        };
        let _ = display::debug_draw_by_date(&draws, date, stdout);
        return 0;
    }

    if parsed.fetch_all {
        return run_fetch_all(paths, transport, stdout, stderr);
    }

    if parsed.show_all {
        return match store::load_draws(paths, stderr) {
            Ok(draws) => {
                let _ = display::display_all_draws(&draws, stdout);
                0
            }
            Err(error) => {
                let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
                1
            }
        };
    }

    if parsed.show_stats {
        return match store::load_draws(paths, stderr) {
            Ok(draws) => {
                let _ = stats::display_statistics(&draws, ColorMode::detect_stdout(), stdout);
                0
            }
            Err(error) => {
                let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
                1
            }
        };
    }

    run_daily(paths, transport, connectivity, terminal, stdout, stderr)
}

fn run_fetch_all<H: HttpTransport, W: Write, E: Write>(
    paths: &store::Paths,
    transport: &H,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    let _ = writeln!(stdout, "Fetching all historical draws...");
    let _ = writeln!(
        stdout,
        "{:<33}  {:>7}  {:>12}",
        "PERIOD", "DRAWS", "GRAND TOTAL"
    );

    let mut existing = match store::load_draws(paths, stderr) {
        Ok(draws) => draws,
        Err(error) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
            return 1;
        }
    };
    let (now_millis, _) = dates::now_or_utc();

    loop {
        let before_count = existing.len();
        let outcome = api::fetch_all_draws_incremental(
            transport,
            existing.clone(),
            now_millis,
            |draws| store::save_draws_callback(paths, draws, &mut *stderr),
            stdout,
        );
        let all_draws = match outcome {
            Ok(draws) => draws,
            Err(error) => {
                let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
                return 1;
            }
        };
        let new_draws_count = all_draws.len() - before_count;
        existing = all_draws;
        if new_draws_count == 0 {
            let _ = writeln!(stdout, "\nNo more historical data available.");
            break;
        }
    }

    let _ = writeln!(
        stdout,
        "\nFetch complete! Total draws in database: {}",
        existing.len()
    );
    0
}

fn dedupe_sorted_by_id(draws: &[Draw]) -> Vec<Draw> {
    let mut sorted: Vec<&Draw> = draws.iter().collect();
    sorted.sort_by_key(|d| d.draw_time);
    let mut seen = HashSet::new();
    sorted
        .into_iter()
        .filter(|d| seen.insert(d.id.clone()))
        .cloned()
        .collect()
}

fn run_daily<H: HttpTransport, C: ConnectivityCheck, T: TerminalCapability, W: Write, E: Write>(
    paths: &store::Paths,
    transport: &H,
    connectivity: &C,
    terminal: &T,
    stdout: &mut W,
    stderr: &mut E,
) -> u8 {
    let mut existing = match store::load_draws(paths, stderr) {
        Ok(draws) => draws,
        Err(error) => {
            let _ = writeln!(stderr, "{PROGRAM_NAME}: {error}");
            return 1;
        }
    };

    let color = ColorMode::detect_stdout();
    let online = connectivity.is_online();
    if !online {
        let _ = writeln!(
            stdout,
            "{}",
            color.paint(RED3, "Internet is unreachable — showing cached data")
        );
    }

    let (now_millis, now_offset) = dates::now_or_utc();

    let mut needs_fetch = false;
    if existing.is_empty() {
        if online {
            let _ = writeln!(
                stdout,
                "Empty local draws.json file. Fetching last 365 drawings..."
            );
            needs_fetch = true;
        }
    } else {
        existing.sort_by_key(|d| d.draw_time);
        let newest_millis = existing.last().expect("non-empty").draw_time;
        let week_ago = now_millis - 7 * 86_400_000;
        if newest_millis < week_ago && online {
            let newest_date = dates::eastern_civil_time(newest_millis).date;
            let _ = writeln!(
                stdout,
                "Data is outdated (newest draw: {}). Fetching recent data...",
                dates::narrative_date(newest_date)
            );
            needs_fetch = true;
        }
    }

    if needs_fetch {
        match api::fetch_all_draws_incremental(
            transport,
            existing.clone(),
            now_millis,
            |draws| store::save_draws_callback(paths, draws, &mut *stderr),
            stdout,
        ) {
            Ok(all) => {
                existing = all;
                let _ = writeln!(stdout);
            }
            Err(error) => {
                let _ = writeln!(stderr, "{PROGRAM_NAME}: failed to fetch draws: {error}");
                return 1;
            }
        }
    }

    if online && !existing.is_empty() {
        existing.sort_by_key(|d| d.draw_time);
        let newest_draw_time = existing.last().expect("non-empty").draw_time;
        let (needs, newest_date, yesterday_date) =
            display::needs_recent_fetch(newest_draw_time, now_millis, now_offset);
        if needs {
            let _ = writeln!(
                stdout,
                "Missing recent draws (newest: {}, need up to: {}). Fetching...",
                dates::narrative_date(newest_date),
                dates::narrative_date(yesterday_date)
            );
            let date_from = dates::days_from_civil(newest_date.add_days(1)) * 86_400_000;
            let date_to = now_millis;
            match api::fetch_draws_by_date_range(
                transport,
                date_from,
                date_to,
                &existing,
                |draws| store::save_draws_callback(paths, draws, &mut *stderr),
                stdout,
            ) {
                Ok(recent) => {
                    existing = recent;
                    let _ = writeln!(
                        stdout,
                        "Fetched recent draws. Total in database: {}\n",
                        existing.len()
                    );
                }
                Err(error) if api::is_404_error(&error) => {
                    let _ = writeln!(
                        stdout,
                        "{}",
                        color.paint(
                            RED3,
                            &format!("Primary source unavailable ({error}) — trying backup...")
                        )
                    );
                    let backup = api::LottoNumbersFetcher { transport };
                    let fetchers: Vec<&dyn DrawFetcher> = vec![&backup];
                    existing = api::try_backup_fetchers(
                        &fetchers,
                        existing,
                        date_from,
                        date_to,
                        |draws| store::save_draws_callback(paths, draws, &mut *stderr),
                        color,
                        stdout,
                    );
                }
                Err(error) => {
                    let _ = writeln!(stdout, "Warning: failed to fetch recent draws: {error}");
                }
            }
        }
    }

    let _ = display::display_last_n_draws(&existing, 10, stdout);

    let unique_draws = dedupe_sorted_by_id(&existing);
    if unique_draws.is_empty() {
        let _ = writeln!(stderr, "{PROGRAM_NAME}: no draws available");
        return 1;
    }

    let mut combo_history: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for draw in &unique_draws {
        if let Ok(mut nums) = strategy::extract_primary_five(draw) {
            nums.sort_unstable();
            let key = format!(
                "{:02}-{:02}-{:02}-{:02}-{:02}",
                nums[0], nums[1], nums[2], nums[3], nums[4]
            );
            let date = dates::narrative_date(dates::eastern_civil_time(draw.draw_time).date);
            combo_history.entry(key).or_default().push(date);
        }
    }

    let last_draw = unique_draws.last().expect("non-empty");
    let mut lwn = match strategy::extract_primary_five(last_draw) {
        Ok(nums) => nums,
        Err(error) => {
            let _ = writeln!(
                stderr,
                "{PROGRAM_NAME}: failed to extract last winning numbers: {error}"
            );
            return 1;
        }
    };
    lwn.sort_unstable();
    let lwn_key = format!(
        "{:02}-{:02}-{:02}-{:02}-{:02}",
        lwn[0], lwn[1], lwn[2], lwn[3], lwn[4]
    );
    let lwn_date = dates::narrative_date(dates::eastern_civil_time(last_draw.draw_time).date);

    if online {
        match api::fetch_current_jackpot(transport) {
            Ok(jackpot) if jackpot > 0 => {
                let _ = writeln!(
                    stdout,
                    "  {}: {}",
                    color.paint(BLUE7, "CURRENT JACKPOT"),
                    color.paint(GREEN3, &strategy::format_currency(jackpot / 100))
                );
            }
            _ => {
                if last_draw.estimated_jackpot > 0 {
                    let _ = writeln!(
                        stdout,
                        "  {}: {}",
                        color.paint(BLUE7, "CURRENT JACKPOT"),
                        color.paint(
                            GREEN3,
                            &strategy::format_currency(last_draw.estimated_jackpot / 100)
                        )
                    );
                }
            }
        }
    } else if last_draw.estimated_jackpot > 0 {
        let _ = writeln!(
            stdout,
            "  {}: {} {}",
            color.paint(BLUE7, "CURRENT JACKPOT"),
            color.paint(
                GREEN3,
                &strategy::format_currency(last_draw.estimated_jackpot / 100)
            ),
            color.paint(GRAY5, "(cached)")
        );
    }

    let _ = write!(
        stdout,
        "  {}: {}",
        color.paint(BLUE7, "LAST WINNING NUMBERS"),
        color.paint(GREEN3, &lwn_key)
    );
    let lwn_dates = combo_history.get(&lwn_key).cloned().unwrap_or_default();
    if lwn_dates.len() > 1 {
        let prior_dates: Vec<&str> = lwn_dates
            .iter()
            .filter(|date| **date != lwn_date)
            .map(String::as_str)
            .collect();
        if !prior_dates.is_empty() {
            let _ = write!(
                stdout,
                "  {}",
                color.paint(BLUE7, &format!("REPEATED: {}", prior_dates.join(", ")))
            );
        } else {
            let _ = write!(stdout, "  {}", color.paint(GRAY5, "Never repeated"));
        }
    } else {
        let _ = write!(stdout, "  {}", color.paint(GRAY5, "Never repeated"));
    }
    let _ = writeln!(stdout);

    if terminal.is_iterm2() {
        let _ = writeln!(stdout, "  {}:", color.paint(BLUE7, "WINNING CIRCLE"));
        let _ = render::display_circle_image(&lwn, "  ", stdout);
    }

    struct CloseMatch {
        draw_time: i64,
        date: String,
        nums: [i32; 5],
        matches: usize,
    }
    let mut close_matches: Vec<CloseMatch> = Vec::new();
    for draw in &unique_draws {
        let draw_date = dates::narrative_date(dates::eastern_civil_time(draw.draw_time).date);
        if draw_date == lwn_date {
            continue;
        }
        if let Ok(mut nums) = strategy::extract_primary_five(draw) {
            nums.sort_unstable();
            let matches = strategy::count_matches(&lwn, &nums);
            if matches >= 3 {
                close_matches.push(CloseMatch {
                    draw_time: draw.draw_time,
                    date: draw_date,
                    nums,
                    matches,
                });
            }
        }
    }
    close_matches.sort_by(|a, b| {
        b.matches
            .cmp(&a.matches)
            .then(b.draw_time.cmp(&a.draw_time))
    });

    let _ = writeln!(
        stdout,
        "  {}:",
        color.paint(BLUE7, "CLOSEST 5 PREVIOUS WINNING MATCHES")
    );
    if close_matches.is_empty() {
        let _ = writeln!(
            stdout,
            "  {}",
            color.paint(GRAY5, "No previous draws with 3+ matching numbers")
        );
    } else {
        let limit = close_matches.len().min(5);
        for cm in &close_matches[..limit] {
            let num_str = format!(
                "{:02}-{:02}-{:02}-{:02}-{:02}",
                cm.nums[0], cm.nums[1], cm.nums[2], cm.nums[3], cm.nums[4]
            );
            let _ = writeln!(
                stdout,
                "    {}  {}  {}",
                color.paint(GREEN3, &num_str),
                color.paint(GREEN3, &cm.date),
                color.paint(GRAY5, &format!("({}/5 match)", cm.matches))
            );
        }
    }

    let winners = recommend::build_winners_set(&unique_draws);
    let recommendations = recommend::generate_recommendations(&unique_draws, &winners, stderr);

    let _ = writeln!(stdout, "  {}:", color.paint(BLUE7, "RECOMMENDATION"));
    let _ = writeln!(
        stdout,
        "    {}",
        color.paint(GRAY5, recommend::RECOMMENDATION_PREAMBLE)
    );
    for rec in &recommendations {
        let num_str = format!(
            "{:02}-{:02}-{:02}-{:02}-{:02}",
            rec.numbers[0], rec.numbers[1], rec.numbers[2], rec.numbers[3], rec.numbers[4]
        );
        let _ = writeln!(
            stdout,
            "    {}  {}",
            color.paint(GREEN3, &num_str),
            color.paint(GRAY5, rec.strategy)
        );
    }

    let _ = writeln!(stdout, "\n  {}", color.paint(RED3, LOTTERY_WARNING));

    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pman::{HttpRequest, HttpResponse};
    use std::io;
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

    struct FakeConnectivity(bool);
    impl ConnectivityCheck for FakeConnectivity {
        fn is_online(&self) -> bool {
            self.0
        }
    }

    struct FakeTerminal(bool);
    impl TerminalCapability for FakeTerminal {
        fn is_iterm2(&self) -> bool {
            self.0
        }
    }

    fn temp_paths() -> (store::Paths, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "rkit-cash5-mod-unit-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        (
            store::Paths {
                home: dir.clone(),
                xdg_state_home: None,
                xdg_config_home: None,
            },
            dir,
        )
    }

    fn no_response_transport() -> FakeTransport {
        FakeTransport {
            response: Mutex::new(Some(Err("no network in test".to_owned()))),
        }
    }

    #[test]
    fn version_flag_prints_exact_line() {
        let (paths, dir) = temp_paths();
        let transport = no_response_transport();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            ["-v"],
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert_eq!(stdout, b"cash5 v1.0.0\n");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn help_flag_prints_usage_not_version() {
        let (paths, dir) = temp_paths();
        let transport = no_response_transport();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            ["-h"],
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.contains("NJ Cash 5 daily numbers recommender"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn unknown_flag_is_rejected() {
        let (paths, dir) = temp_paths();
        let transport = no_response_transport();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            ["--bogus"],
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 1);
        assert!(String::from_utf8(stderr).unwrap().contains("unknown flag"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn stats_flag_on_empty_store_reports_no_draws() {
        let (paths, dir) = temp_paths();
        let transport = no_response_transport();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            ["-s"],
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert_eq!(String::from_utf8(stdout).unwrap(), "No draws found\n");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn odds_table_default_n_is_thirty_rows() {
        let (paths, dir) = temp_paths();
        let transport = no_response_transport();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            ["-o"],
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        let text = String::from_utf8(stdout).unwrap();
        assert_eq!(text.lines().filter(|l| l.contains("1 in")).count(), 30);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn odds_table_explicit_n_overrides_default() {
        let (paths, dir) = temp_paths();
        let transport = no_response_transport();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            ["-o", "3"],
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        let text = String::from_utf8(stdout).unwrap();
        assert_eq!(text.lines().filter(|l| l.contains("1 in")).count(), 3);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn match_analysis_flag_dispatches_with_default_and_explicit_n() {
        let (paths, dir) = temp_paths();
        let transport = no_response_transport();

        // Default N (no value supplied) on an empty store reports no draws
        // rather than erroring, confirming -m alone is accepted.
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            ["-m"],
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert_eq!(String::from_utf8(stdout).unwrap(), "No draws found\n");

        // Explicit N is honored via the long-form flag too.
        let mut stdout = Vec::new();
        let code = run_with(
            ["--match-analysis", "10"],
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert_eq!(String::from_utf8(stdout).unwrap(), "No draws found\n");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn debug_date_on_empty_store_reports_no_match() {
        let (paths, dir) = temp_paths();
        let transport = no_response_transport();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            ["-d", "2026-02-06"],
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0);
        assert!(
            String::from_utf8(stdout)
                .unwrap()
                .contains("No draw found for date 2026-02-06")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn seed_draws(paths: &store::Paths, n: i64) {
        let mut seed = Vec::new();
        let base = store::CASH5_ERA_START_MILLIS;
        for i in 0..n {
            let combo = [
                1 + i % 45,
                1 + (i * 2 + 7) % 45,
                1 + (i * 3 + 13) % 45,
                1 + (i * 5 + 19) % 45,
                1 + (i * 7 + 29) % 45,
            ];
            let mut nums = combo;
            let mut seen = HashSet::new();
            for slot in nums.iter_mut() {
                while seen.contains(slot) {
                    *slot = *slot % 45 + 1;
                }
                seen.insert(*slot);
            }
            seed.push(Draw {
                id: format!("d{i}"),
                game_name: "Cash 5".to_owned(),
                draw_time: base + i * 86_400_000,
                results: vec![model::DrawResult {
                    primary: nums.iter().map(|n| n.to_string()).collect(),
                    ..Default::default()
                }],
                ..Default::default()
            });
        }
        let mut setup_stderr = Vec::new();
        store::save_draws_callback(paths, &seed, &mut setup_stderr).unwrap();
    }

    #[test]
    fn daily_run_offline_with_seeded_data_recommends_without_network() {
        let (paths, dir) = temp_paths();
        seed_draws(&paths, 40);

        let transport = no_response_transport();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            Vec::<&str>::new(),
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(false),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.contains("RECOMMENDATION"));
        assert!(text.contains("Internet is unreachable"));
        assert!(!text.contains("WINNING CIRCLE"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn daily_run_emits_winning_circle_only_when_terminal_reports_iterm2() {
        let (paths, dir) = temp_paths();
        seed_draws(&paths, 40);

        let transport = no_response_transport();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let code = run_with(
            Vec::<&str>::new(),
            "1.0.0",
            &paths,
            &transport,
            &FakeConnectivity(false),
            &FakeTerminal(true),
            &mut stdout,
            &mut stderr,
        );
        assert_eq!(code, 0, "stderr: {}", String::from_utf8_lossy(&stderr));
        let text = String::from_utf8(stdout).unwrap();
        assert!(text.contains("WINNING CIRCLE"));
        assert!(text.contains("\x1b]1337;File=inline=1;preserveAspectRatio=1:"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
