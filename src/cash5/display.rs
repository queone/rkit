//! Draw listing, debug dump, odds table, and fetch-trigger display logic
//! for `cash5`, ported from Go's `display.go` and the fetch-trigger
//! helper in `main.go`.

use crate::cash5::dates::{self, CivilDate};
use crate::cash5::model::Draw;
use crate::cash5::stats::format_number;
use crate::cash5::strategy::{extract_primary_five, format_payout};
use std::collections::HashSet;
use std::io::{self, Write};

/// Prints a `2026-feb-17 Tue 08:47p`-shaped header line in the operator's
/// current local time (resolved via `date`, falling back to UTC).
pub fn print_timestamp<W: Write>(out: &mut W) -> io::Result<()> {
    let (millis, offset) = dates::now_or_utc();
    let civil = dates::civil_time_at_offset(millis, offset);
    let weekday = dates::weekday_abbrev(dates::days_from_civil(civil.date));
    let hour12 = if civil.hour % 12 == 0 {
        12
    } else {
        civil.hour % 12
    };
    let ampm = if civil.hour >= 12 { "p" } else { "a" };
    writeln!(
        out,
        "{} {} {:02}:{:02}{}",
        dates::narrative_date(civil.date),
        weekday,
        hour12,
        civil.min,
        ampm
    )
}

fn dedupe_sorted_by_time(draws: &[Draw]) -> Vec<Draw> {
    let mut sorted: Vec<&Draw> = draws.iter().collect();
    sorted.sort_by_key(|d| d.draw_time);
    let mut seen = HashSet::new();
    sorted
        .into_iter()
        .filter(|d| seen.insert(d.id.clone()))
        .cloned()
        .collect()
}

fn print_draw_table_row<W: Write>(out: &mut W, draw: &Draw) -> io::Result<()> {
    let Ok(nums) = extract_primary_five(draw) else {
        return Ok(());
    };
    let draw_date = dates::ymd(dates::eastern_civil_time(draw.draw_time).date);
    let num_str = format!(
        "{:02}-{:02}-{:02}-{:02}-{:02}",
        nums[0], nums[1], nums[2], nums[3], nums[4]
    );
    let payout = format_payout(draw);
    writeln!(out, "  {draw_date:<12}  {num_str:<20}  {payout:>15}")
}

pub fn display_all_draws<W: Write>(draws: &[Draw], out: &mut W) -> io::Result<()> {
    if draws.is_empty() {
        return writeln!(out, "No draws found");
    }
    let unique = dedupe_sorted_by_time(draws);
    print_timestamp(out)?;
    writeln!(
        out,
        "{:<12}  {:<20}  {:>15}",
        "DATE", "WINNING NUMBERS", "5/5 PAYOUT"
    )?;
    for draw in &unique {
        print_draw_table_row(out, draw)?;
    }
    Ok(())
}

pub fn display_last_n_draws<W: Write>(draws: &[Draw], n: usize, out: &mut W) -> io::Result<()> {
    if draws.is_empty() {
        return writeln!(out, "No draws found");
    }
    let unique = dedupe_sorted_by_time(draws);
    let start = unique.len().saturating_sub(n);
    print_timestamp(out)?;
    writeln!(
        out,
        "  {:<12}  {:<20}  {:>15}",
        "DATE", "WINNING NUMBERS", "5/5 PAYOUT"
    )?;
    for draw in &unique[start..] {
        print_draw_table_row(out, draw)?;
    }
    Ok(())
}

/// Parses `YYYY-MM-DD` and dumps every field of every draw on that date
/// (Eastern-Time calendar day), for the `-d`/`--debug` flag.
pub fn debug_draw_by_date<W: Write>(draws: &[Draw], date_str: &str, out: &mut W) -> io::Result<()> {
    let target = match parse_ymd(date_str) {
        Some(date) => date,
        None => return writeln!(out, "invalid date format, use YYYY-MM-DD"),
    };
    let unique = dedupe_sorted_by_time(draws);
    let mut found = false;
    for draw in &unique {
        let draw_date = dates::eastern_civil_time(draw.draw_time).date;
        if draw_date == target {
            found = true;
            writeln!(out, "Draw for {}:", dates::ymd(draw_date))?;
            writeln!(out, "ID: {}", draw.id)?;
            writeln!(out, "GameName: {}", draw.game_name)?;
            writeln!(out, "Status: {}", draw.status)?;
            writeln!(
                out,
                "EstimatedJackpot: {} (= ${})",
                draw.estimated_jackpot,
                draw.estimated_jackpot / 100
            )?;
            writeln!(out, "Jackpot: {}", draw.jackpot)?;
            writeln!(out, "\nResults (count: {}):", draw.results.len())?;
            for (i, r) in draw.results.iter().enumerate() {
                writeln!(out, "  [{i}] DrawType: {}", r.draw_type)?;
                writeln!(out, "      Primary: {:?}", r.primary)?;
                writeln!(
                    out,
                    "      PrimaryRevealOrder: {:?}",
                    r.primary_reveal_order
                )?;
                writeln!(out, "      Winners: {}", r.winners)?;
                writeln!(out, "      Payout: {}", r.payout)?;
                writeln!(out, "      PrizeAmount: {}", r.prize_amount)?;
            }
            writeln!(out, "\nPrizeTiers (count: {}):", draw.prize_tiers.len())?;
            if draw.prize_tiers.is_empty() {
                writeln!(out, "  (empty)")?;
            }
            for (i, pt) in draw.prize_tiers.iter().enumerate() {
                writeln!(
                    out,
                    "  [{i}] Tier: {}, Match: {}, Winners: {}",
                    pt.tier, pt.match_tier, pt.winners
                )?;
                writeln!(
                    out,
                    "      PrizeAmount: {}, Prize: {}",
                    pt.prize_amount, pt.prize
                )?;
                writeln!(out, "      Description: {}", pt.description)?;
            }
            writeln!(out, "\nPrizes (count: {}):", draw.prizes.len())?;
            if draw.prizes.is_empty() {
                writeln!(out, "  (empty)")?;
            }
            for (i, p) in draw.prizes.iter().enumerate() {
                writeln!(
                    out,
                    "  [{i}] Level: {}, Winners: {}, Amount: {}",
                    p.level, p.winners, p.amount
                )?;
                writeln!(out, "      Description: {}", p.description)?;
            }
            writeln!(
                out,
                "\nWinningNumbers: {}",
                draw.winning_numbers
                    .as_ref()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "<nil>".to_owned())
            )?;
            writeln!(out, "\n{}", "-".repeat(60))?;
        }
    }
    if !found {
        writeln!(out, "No draw found for date {date_str}")?;
    }
    Ok(())
}

fn parse_ymd(s: &str) -> Option<CivilDate> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year = parts[0].parse().ok()?;
    let month = parts[1].parse().ok()?;
    let day = parts[2].parse().ok()?;
    Some(CivilDate { year, month, day })
}

pub fn format_ev(ev: f64) -> String {
    if ev >= 0.0 {
        format!("+${ev:.2}")
    } else {
        format!("-${:.2}", -ev)
    }
}

pub fn format_probability(pct: f64) -> String {
    if pct >= 1.0 {
        format!("{pct:.4}%")
    } else {
        format!("{pct:.6}%")
    }
}

/// Prints an odds table for 1 to `max_combos` combos played. `jackpot_dollars`
/// is resolved by the caller (a live fetch with a cached-estimate fallback,
/// matching Go's `displayOddsTable` -- kept out of this pure-display
/// function to isolate the HTTP concern in `api.rs`).
pub fn display_odds_table<W: Write>(
    max_combos: i64,
    jackpot_dollars: Option<i64>,
    out: &mut W,
) -> io::Result<()> {
    const TOTAL_COMBOS: i64 = 1_221_759;
    const TICKET_COST: i64 = 2;

    writeln!(
        out,
        "NJ Cash 5 Odds Table. Total possible combinations: {} (C(45,5))",
        format_number(TOTAL_COMBOS)
    )?;
    if let Some(dollars) = jackpot_dollars.filter(|d| *d > 0) {
        writeln!(
            out,
            "Current jackpot: {}",
            crate::cash5::strategy::format_currency(dollars)
        )?;
    }

    let max_combos = max_combos.min(TOTAL_COMBOS);

    if jackpot_dollars.is_some_and(|d| d > 0) {
        writeln!(
            out,
            "{:<9}  {:>5}    {:<16}  {:<14}  {:>6}",
            "COMBOS", "COST", "ODDS", "PROBABILITY", "EV"
        )?;
    } else {
        writeln!(
            out,
            "{:<9}  {:>5}    {:<16}  PROBABILITY",
            "COMBOS", "COST", "ODDS"
        )?;
    }

    for n in 1..=max_combos {
        let cost = n * TICKET_COST;
        let one_in_x = (TOTAL_COMBOS + n - 1) / n;
        let prob = n as f64 / TOTAL_COMBOS as f64;

        if let Some(dollars) = jackpot_dollars.filter(|d| *d > 0) {
            let ev = prob * dollars as f64 - cost as f64;
            writeln!(
                out,
                "{n:6}     {:>5}    1 in {:<11}  {:<14}  {:>6}",
                crate::cash5::strategy::format_currency(cost),
                format_number(one_in_x),
                format_probability(prob * 100.0),
                format_ev(ev)
            )?;
        } else {
            writeln!(
                out,
                "{n:6}     {:>5}    1 in {:<11}  {}%",
                crate::cash5::strategy::format_currency(cost),
                format_number(one_in_x),
                format_probability(prob * 100.0)
            )?;
        }
    }
    Ok(())
}

/// Reports whether the newest cached `draw_time` is at least one full
/// calendar day older than `now` in `now`'s timezone (given as an explicit
/// UTC offset). Returns the trigger decision plus the newest and yesterday
/// dates in that same offset, so the caller can format the user-facing
/// message and bound the fetch window.
pub fn needs_recent_fetch(
    newest_draw_time_millis: i64,
    now_millis: i64,
    now_offset_seconds: i64,
) -> (bool, CivilDate, CivilDate) {
    let newest = dates::civil_time_at_offset(newest_draw_time_millis, now_offset_seconds).date;
    let today = dates::civil_time_at_offset(now_millis, now_offset_seconds).date;
    let yesterday = today.add_days(-1);
    let needs = dates::days_from_civil(newest) < dates::days_from_civil(yesterday);
    (needs, newest, yesterday)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_midnight_millis(date: CivilDate, offset_seconds: i64) -> i64 {
        dates::days_from_civil(date) * 86_400_000 - offset_seconds * 1000
    }

    const EDT: i64 = -4 * 3600;
    const PDT: i64 = -7 * 3600;
    const CEST: i64 = 2 * 3600;

    #[test]
    fn needs_recent_fetch_no_fire_on_todays_draw() {
        let now = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            EDT,
        ) + 20 * 3600
            + 47 * 60_000;
        let newest = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            EDT,
        );
        let (got, newest_date, _) = needs_recent_fetch(newest, now, EDT);
        assert!(!got);
        assert_eq!(
            newest_date,
            CivilDate {
                year: 2026,
                month: 5,
                day: 13
            }
        );
    }

    #[test]
    fn needs_recent_fetch_no_fire_on_yesterdays_draw() {
        let now = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 14,
            },
            EDT,
        ) + 14 * 3600 * 1000;
        let newest = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            EDT,
        );
        let (got, _, _) = needs_recent_fetch(newest, now, EDT);
        assert!(!got);
    }

    #[test]
    fn needs_recent_fetch_fires_when_two_days_old() {
        let now = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 15,
            },
            EDT,
        ) + 14 * 3600 * 1000;
        let newest = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            EDT,
        );
        let (got, _, yesterday) = needs_recent_fetch(newest, now, EDT);
        assert!(got);
        assert_eq!(
            yesterday,
            CivilDate {
                year: 2026,
                month: 5,
                day: 14
            }
        );
    }

    #[test]
    fn needs_recent_fetch_honors_operator_tz_west() {
        let now = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            PDT,
        ) + 9 * 3600 * 1000;
        let newest = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            PDT,
        );
        let (got, _, _) = needs_recent_fetch(newest, now, PDT);
        assert!(!got);
    }

    #[test]
    fn needs_recent_fetch_honors_operator_tz_east() {
        let now = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            CEST,
        ) + 10 * 3600 * 1000;
        let newest = local_midnight_millis(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            CEST,
        );
        let (got, _, _) = needs_recent_fetch(newest, now, CEST);
        assert!(!got);
    }

    #[test]
    fn format_ev_signs_correctly() {
        assert_eq!(format_ev(1.5), "+$1.50");
        assert_eq!(format_ev(-1.5), "-$1.50");
    }

    #[test]
    fn format_probability_switches_precision_at_one_percent() {
        assert_eq!(format_probability(5.0), "5.0000%");
        assert_eq!(format_probability(0.5), "0.500000%");
    }

    #[test]
    fn display_odds_table_without_jackpot_omits_ev_column() {
        let mut out = Vec::new();
        display_odds_table(3, None, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("EV"));
        assert!(text.contains("PROBABILITY"));
    }

    #[test]
    fn display_odds_table_with_jackpot_includes_ev() {
        let mut out = Vec::new();
        display_odds_table(2, Some(1_000_000), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("EV"));
        assert!(text.contains("Current jackpot"));
    }

    #[test]
    fn parse_ymd_rejects_malformed_input() {
        assert!(parse_ymd("2026-02-06").is_some());
        assert!(parse_ymd("bad-date").is_none());
    }
}
