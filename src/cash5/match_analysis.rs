//! Per-draw closest-match analysis and cross-dataset pattern analysis for
//! `cash5`, ported from Go's `match.go`.

use crate::cash5::dates;
use crate::cash5::display::print_timestamp;
use crate::cash5::model::Draw;
use crate::cash5::render::{self, TerminalCapability};
use crate::cash5::strategy::{count_matches, extract_primary_five, format_payout};
use crate::color::{BLUE7, ColorMode, GRAY5, GREEN3};
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};

struct ClosestMatch {
    date: String,
    nums: [i32; 5],
    matches: usize,
    days_delta: i64,
}

fn quintile(n: i32) -> i32 {
    (n - 1) / 9
}

fn recency_bucket_name(delta_days: i64) -> &'static str {
    match delta_days {
        d if d <= 90 => "0-90 days",
        d if d <= 365 => "91-365 days",
        d if d <= 1095 => "1-3 years",
        _ => "3+ years",
    }
}

#[derive(Default)]
struct RecencyData {
    matches: i64,
    candidates: i64,
}

/// Renders per-draw closest-match blocks (windowed to the last `n`
/// drawings) followed by the cross-dataset pattern-analysis sections,
/// matching Go's `displayMatchAnalysis`.
pub(crate) fn display_match_analysis<T: TerminalCapability, W: Write>(
    draws: &[Draw],
    n: usize,
    color: ColorMode,
    terminal: &T,
    out: &mut W,
) -> io::Result<()> {
    if draws.is_empty() {
        return writeln!(out, "No draws found");
    }

    let mut sorted: Vec<&Draw> = draws.iter().collect();
    sorted.sort_by_key(|d| d.draw_time);
    let mut seen = HashSet::new();
    let unique: Vec<&Draw> = sorted
        .into_iter()
        .filter(|d| seen.insert(d.id.clone()))
        .collect();

    struct Parsed<'a> {
        draw: &'a Draw,
        nums: [i32; 5],
        date: String,
    }
    let parsed: Vec<Parsed> = unique
        .iter()
        .filter_map(|draw| {
            extract_primary_five(draw).ok().map(|mut nums| {
                nums.sort_unstable();
                Parsed {
                    draw,
                    nums,
                    date: dates::narrative_date(dates::eastern_civil_time(draw.draw_time).date),
                }
            })
        })
        .collect();

    if parsed.is_empty() {
        return writeln!(out, "No valid draws found");
    }

    let display_start = parsed.len().saturating_sub(n).max(1);

    print_timestamp(out)?;
    writeln!(out, "=== MATCH ANALYSIS ===")?;
    if n < parsed.len() {
        writeln!(
            out,
            "Analyzing {} drawings (of {} total) for closest historical matches...\n",
            parsed.len() - display_start,
            parsed.len()
        )?;
    } else {
        writeln!(
            out,
            "Analyzing {} drawings for closest historical matches...\n",
            parsed.len()
        )?;
    }

    let mut match_num_freq: HashMap<i32, i64> = HashMap::new();
    let mut quintile_match_same = 0i64;
    let mut quintile_match_total = 0i64;
    let mut recency_buckets: HashMap<&str, RecencyData> =
        ["0-90 days", "91-365 days", "1-3 years", "3+ years"]
            .into_iter()
            .map(|name| (name, RecencyData::default()))
            .collect();
    let mut first_half_dist = [0i64; 6];
    let mut second_half_dist = [0i64; 6];
    let mut match_pair_freq: HashMap<String, i64> = HashMap::new();
    let mut total_match_entries = 0i64;

    for i in 1..parsed.len() {
        let current = &parsed[i];
        let current_time = current.draw.draw_time;

        let mut matches: Vec<ClosestMatch> = (0..i)
            .map(|j| {
                let prev = &parsed[j];
                let days_delta = (current_time - prev.draw.draw_time).abs() / 86_400_000;
                ClosestMatch {
                    date: prev.date.clone(),
                    nums: prev.nums,
                    matches: count_matches(&current.nums, &prev.nums),
                    days_delta,
                }
            })
            .collect();
        matches.sort_by(|a, b| {
            b.matches
                .cmp(&a.matches)
                .then(a.days_delta.cmp(&b.days_delta))
        });
        let limit = matches.len().min(10);
        let top_matches = &matches[..limit];

        if i >= display_start {
            let payout = format_payout(current.draw);
            let num_str = format!(
                "{:02}-{:02}-{:02}-{:02}-{:02}",
                current.nums[0], current.nums[1], current.nums[2], current.nums[3], current.nums[4]
            );
            writeln!(
                out,
                "{}  {}  5/5 {}",
                color.paint(GREEN3, &current.date),
                color.paint(GREEN3, &num_str),
                color.paint(GRAY5, &payout)
            )?;
            for m in top_matches {
                let m_num_str = format!(
                    "{:02}-{:02}-{:02}-{:02}-{:02}",
                    m.nums[0], m.nums[1], m.nums[2], m.nums[3], m.nums[4]
                );
                writeln!(
                    out,
                    "    {}  {}  {}",
                    color.paint(GREEN3, &m_num_str),
                    color.paint(GRAY5, &m.date),
                    color.paint(
                        GRAY5,
                        &format!("({}/5 match, {} days prior)", m.matches, m.days_delta)
                    )
                )?;
            }
            if terminal.is_iterm2() {
                render::display_circle_image(&current.nums, "    ", out)?;
            }
            writeln!(out)?;
        }

        for prev in parsed.iter().take(i) {
            let delta = (current_time - prev.draw.draw_time).abs() / 86_400_000;
            recency_buckets
                .get_mut(recency_bucket_name(delta))
                .unwrap()
                .candidates += 1;
        }

        let is_second_half = i >= parsed.len() / 2;

        for m in top_matches {
            total_match_entries += 1;
            for &num in &m.nums {
                *match_num_freq.entry(num).or_insert(0) += 1;
            }
            if is_second_half {
                second_half_dist[m.matches] += 1;
            } else {
                first_half_dist[m.matches] += 1;
            }
            recency_buckets
                .get_mut(recency_bucket_name(m.days_delta))
                .unwrap()
                .matches += 1;
            for pi in 0..m.nums.len() - 1 {
                for pj in pi + 1..m.nums.len() {
                    let pair = format!("{:02}-{:02}", m.nums[pi], m.nums[pj]);
                    *match_pair_freq.entry(pair).or_insert(0) += 1;
                }
            }
            let matched: HashSet<i32> = current
                .nums
                .iter()
                .filter(|cn| m.nums.contains(cn))
                .copied()
                .collect();
            let matched_vec: Vec<i32> = matched.into_iter().collect();
            for a in &matched_vec {
                for b in &matched_vec {
                    if a < b {
                        quintile_match_total += 1;
                        if quintile(*a) == quintile(*b) {
                            quintile_match_same += 1;
                        }
                    }
                }
            }
        }
    }

    writeln!(out, "=== PATTERN ANALYSIS ===")?;
    writeln!(out)?;

    // 1. Number frequency in top matches vs uniform 1-45 baseline.
    writeln!(
        out,
        "{}:",
        color.paint(BLUE7, "Number Frequency in Top Matches")
    )?;
    let total_match_nums = total_match_entries * 5;
    let flat_expected = total_match_nums as f64 / 45.0;
    let mut residuals: Vec<(i32, i64, f64)> = Vec::new();
    let mut variance = 0.0;
    for num in 1..=45 {
        let observed = *match_num_freq.get(&num).unwrap_or(&0);
        let residual = observed as f64 - flat_expected;
        variance += residual * residual;
        residuals.push((num, observed, residual));
    }
    let stddev = (variance / residuals.len() as f64).sqrt();
    let mut over_rep: Vec<(i32, i64)> = residuals
        .iter()
        .filter(|(_, _, r)| *r > 2.0 * stddev)
        .map(|(n, o, _)| (*n, *o))
        .collect();
    let mut under_rep: Vec<(i32, i64)> = residuals
        .iter()
        .filter(|(_, _, r)| *r < -2.0 * stddev)
        .map(|(n, o, _)| (*n, *o))
        .collect();
    over_rep.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    under_rep.sort_by_key(|entry| entry.1);

    writeln!(
        out,
        "  {}: {:.1}",
        color.paint(BLUE7, "Adjusted std dev"),
        stddev
    )?;
    if over_rep.is_empty() {
        writeln!(
            out,
            "  {}",
            color.paint(GRAY5, "No numbers significantly over-represented")
        )?;
    } else {
        writeln!(
            out,
            "  {}:",
            color.paint(BLUE7, "Over-represented (>2σ above adjusted expected)")
        )?;
        for (num, observed) in &over_rep {
            writeln!(
                out,
                "    {}: {}  {}",
                color.paint(GREEN3, &format!("{num:02}")),
                color.paint(GREEN3, &format!("{observed} times")),
                color.paint(GRAY5, &format!("(adjusted expected ~{flat_expected:.0})"))
            )?;
        }
    }
    if under_rep.is_empty() {
        writeln!(
            out,
            "  {}",
            color.paint(GRAY5, "No numbers significantly under-represented")
        )?;
    } else {
        writeln!(
            out,
            "  {}:",
            color.paint(BLUE7, "Under-represented (>2σ below adjusted expected)")
        )?;
        for (num, observed) in &under_rep {
            writeln!(
                out,
                "    {}: {}  {}",
                color.paint(GREEN3, &format!("{num:02}")),
                color.paint(GREEN3, &format!("{observed} times")),
                color.paint(GRAY5, &format!("(adjusted expected ~{flat_expected:.0})"))
            )?;
        }
    }

    // 2. Value-range quintile clustering.
    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Value-Range Clustering (Quintile Analysis)")
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(
            GRAY5,
            "How often do matched numbers fall in the same quintile of the pool range?"
        )
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(GRAY5, "Quintiles: 1-9, 10-18, 19-27, 28-36, 37-45")
    )?;
    if quintile_match_total > 0 {
        let same_rate = quintile_match_same as f64 / quintile_match_total as f64 * 100.0;
        let expected_rate = 20.0;
        writeln!(
            out,
            "    {}: {}  {}",
            color.paint(BLUE7, "Same-quintile pair rate"),
            color.paint(GREEN3, &format!("{same_rate:.1}%")),
            color.paint(
                GRAY5,
                &format!("({quintile_match_same}/{quintile_match_total} pairs)")
            )
        )?;
        writeln!(
            out,
            "    {}: {}",
            color.paint(BLUE7, "Expected if random"),
            color.paint(GREEN3, &format!("{expected_rate:.1}%"))
        )?;
        let deviation = same_rate - expected_rate;
        if deviation.abs() < 3.0 {
            writeln!(
                out,
                "    {}: {}",
                color.paint(BLUE7, "Assessment"),
                color.paint(
                    GREEN3,
                    &format!("Within expected range ({deviation:.1}% deviation)")
                )
            )?;
        } else if deviation > 0.0 {
            writeln!(
                out,
                "    {}: {}",
                color.paint(BLUE7, "Assessment"),
                color.paint(
                    GRAY5,
                    &format!("Matched numbers cluster in same value range (+{deviation:.1}%)")
                )
            )?;
        } else {
            writeln!(
                out,
                "    {}: {}",
                color.paint(BLUE7, "Assessment"),
                color.paint(
                    GRAY5,
                    &format!("Matched numbers spread across value ranges ({deviation:.1}%)")
                )
            )?;
        }
    } else {
        writeln!(
            out,
            "    {}",
            color.paint(GRAY5, "Insufficient matched-number pairs for analysis")
        )?;
    }

    // 3. Recency weighting (density-normalized).
    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Recency Weighting (Density-Normalized)")
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(
            GRAY5,
            "Match density = matches per candidate draw in each time window"
        )
    )?;
    for bucket in ["0-90 days", "91-365 days", "1-3 years", "3+ years"] {
        let rd = &recency_buckets[bucket];
        let density = if rd.candidates > 0 {
            rd.matches as f64 / rd.candidates as f64 * 1000.0
        } else {
            0.0
        };
        writeln!(
            out,
            "    {}: {}  {}  {}",
            color.paint(BLUE7, &format!("{bucket:<13}")),
            color.paint(GREEN3, &format!("{} matches", rd.matches)),
            color.paint(GRAY5, &format!("/ {} candidates", rd.candidates)),
            color.paint(GREEN3, &format!("({density:.2} per 1k)"))
        )?;
    }

    // 4. Match distribution shift over time.
    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Match Distribution Shift Over Time")
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(
            GRAY5,
            "How top-10 match overlap changes as the candidate pool grows"
        )
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(GRAY5, "(first half of dataset vs second half)")
    )?;
    let first_total: i64 = first_half_dist.iter().sum();
    let second_total: i64 = second_half_dist.iter().sum();
    writeln!(
        out,
        "\n    {:<5}  {:>12}  {:>12}",
        "MATCH", "FIRST HALF", "SECOND HALF"
    )?;
    for k in 0..6 {
        let fp = if first_total > 0 {
            first_half_dist[k] as f64 / first_total as f64 * 100.0
        } else {
            0.0
        };
        let sp = if second_total > 0 {
            second_half_dist[k] as f64 / second_total as f64 * 100.0
        } else {
            0.0
        };
        writeln!(
            out,
            "    {k}/5    {}  {}",
            color.paint(GREEN3, &format!("{:5} ({:5.1}%)", first_half_dist[k], fp)),
            color.paint(GREEN3, &format!("{:5} ({:5.1}%)", second_half_dist[k], sp))
        )?;
    }
    if first_total > 0 && second_total > 0 {
        let mut first_avg = 0.0;
        let mut second_avg = 0.0;
        for k in 0..6 {
            first_avg += k as f64 * first_half_dist[k] as f64;
            second_avg += k as f64 * second_half_dist[k] as f64;
        }
        first_avg /= first_total as f64;
        second_avg /= second_total as f64;
        writeln!(
            out,
            "    {}: {first_avg:.3} → {second_avg:.3}",
            color.paint(BLUE7, "Avg match count")
        )?;
        if second_avg > first_avg + 0.05 {
            writeln!(
                out,
                "    {}",
                color.paint(
                    GRAY5,
                    "Higher overlap in second half — larger pool yields closer matches"
                )
            )?;
        } else if first_avg > second_avg + 0.05 {
            writeln!(
                out,
                "    {}",
                color.paint(
                    GRAY5,
                    "Lower overlap in second half — possible pool diversification"
                )
            )?;
        } else {
            writeln!(
                out,
                "    {}",
                color.paint(GRAY5, "Match overlap stable across dataset")
            )?;
        }
    }

    // 5. Top pairs with lift score (PMI).
    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Top Pairs in Closest Matches (Lift-Adjusted)")
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(
            GRAY5,
            "Lift = observed co-occurrence / expected from individual frequencies"
        )
    )?;
    let mut pair_lifts: Vec<(String, i64, f64, f64)> = Vec::new();
    if total_match_entries > 0 {
        let pair_slots = total_match_entries as f64 * 10.0;
        const P_UNIFORM: f64 = 1.0 / 45.0;
        let expected_count = pair_slots * P_UNIFORM * P_UNIFORM;
        for (pair, &count) in &match_pair_freq {
            let lift = if expected_count > 0.0 {
                count as f64 / expected_count
            } else {
                0.0
            };
            pair_lifts.push((pair.clone(), count, lift, expected_count));
        }
    }
    pair_lifts.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let mut shown = 0;
    for (pair, count, lift, expected) in &pair_lifts {
        if *count < 5 {
            continue;
        }
        writeln!(
            out,
            "    {}: {}  {}  {}",
            color.paint(GREEN3, pair),
            color.paint(GREEN3, &format!("{count} times")),
            color.paint(GRAY5, &format!("(expected {expected:.1})")),
            color.paint(GREEN3, &format!("lift {lift:.2}x"))
        )?;
        shown += 1;
        if shown >= 10 {
            break;
        }
    }
    if shown == 0 {
        writeln!(
            out,
            "    {}",
            color.paint(GRAY5, "Insufficient pair data for lift analysis")
        )?;
    }

    if parsed.len() < 50 {
        writeln!(
            out,
            "\n{}",
            color.paint(
                GRAY5,
                "Note: fewer than 50 draws — pattern analysis may be unreliable"
            )
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cash5::model::DrawResult;
    use regex::Regex;

    struct FakeTerminal(bool);
    impl TerminalCapability for FakeTerminal {
        fn is_iterm2(&self) -> bool {
            self.0
        }
    }

    fn synthesize_draws(n: usize) -> Vec<Draw> {
        let base = 1_772_236_800_000i64; // 2026-03-01 00:00 UTC
        (0..n)
            .map(|i| {
                let mut nums = [
                    1 + i as i32 % 45,
                    1 + (i * 2 + 7) as i32 % 45,
                    1 + (i * 3 + 13) as i32 % 45,
                    1 + (i * 5 + 19) as i32 % 45,
                    1 + (i * 7 + 29) as i32 % 45,
                ];
                let mut seen = HashSet::new();
                for slot in nums.iter_mut() {
                    while seen.contains(slot) {
                        *slot = *slot % 45 + 1;
                    }
                    seen.insert(*slot);
                }
                Draw {
                    id: format!("syn-{i:04}"),
                    game_name: "Cash 5".to_owned(),
                    draw_time: base - i as i64 * 86_400_000,
                    results: vec![DrawResult {
                        primary: nums.iter().map(|n| n.to_string()).collect(),
                        ..Default::default()
                    }],
                    ..Default::default()
                }
            })
            .collect()
    }

    fn count_per_draw_blocks(out: &str) -> usize {
        let pattern =
            Regex::new(r"(?m)^\d{4}-[a-z]{3}-\d{2}\s+\d{2}-\d{2}-\d{2}-\d{2}-\d{2}\s+5/5").unwrap();
        pattern.find_iter(out).count()
    }

    #[test]
    fn windows_display_loop() {
        let draws = synthesize_draws(40);
        let color = ColorMode::new(false);
        for (n, want) in [(5, 5), (30, 30), (100, 39)] {
            let mut out = Vec::new();
            display_match_analysis(&draws, n, color, &FakeTerminal(false), &mut out).unwrap();
            let text = String::from_utf8(out).unwrap();
            assert_eq!(count_per_draw_blocks(&text), want, "n={n}");
        }
    }

    #[test]
    fn pattern_sections_always_present() {
        let draws = synthesize_draws(40);
        let color = ColorMode::new(false);
        let mut out = Vec::new();
        display_match_analysis(&draws, 5, color, &FakeTerminal(false), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        for section in [
            "PATTERN ANALYSIS",
            "Number Frequency in Top Matches",
            "Recency Weighting",
            "Match Distribution Shift Over Time",
            "Top Pairs in Closest Matches",
        ] {
            assert!(text.contains(section), "missing section {section}");
        }
    }

    #[test]
    fn header_phrasing_windowed_vs_unwindowed() {
        let draws = synthesize_draws(40);
        let color = ColorMode::new(false);

        let mut out = Vec::new();
        display_match_analysis(&draws, 5, color, &FakeTerminal(false), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Analyzing 5 drawings (of 40 total) for closest"));

        let mut out = Vec::new();
        display_match_analysis(&draws, 40, color, &FakeTerminal(false), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("Analyzing 40 drawings for closest"));
        assert!(!text.contains("(of"));
    }

    #[test]
    fn per_draw_circle_emitted_only_when_terminal_reports_iterm2() {
        let draws = synthesize_draws(5);
        let color = ColorMode::new(false);

        let mut without = Vec::new();
        display_match_analysis(&draws, 5, color, &FakeTerminal(false), &mut without).unwrap();
        let without_text = String::from_utf8(without).unwrap();
        assert!(!without_text.contains("\x1b]1337;File=inline=1;"));

        let mut with = Vec::new();
        display_match_analysis(&draws, 5, color, &FakeTerminal(true), &mut with).unwrap();
        let with_text = String::from_utf8(with).unwrap();
        // 5 draws -> 4 per-draw blocks (entry 0 has no prior draws to
        // compare), so 4 emitted circle images.
        assert_eq!(with_text.matches("\x1b]1337;File=inline=1;").count(), 4);
    }

    #[test]
    fn quintile_maps_ranges_correctly() {
        assert_eq!(quintile(1), 0);
        assert_eq!(quintile(9), 0);
        assert_eq!(quintile(10), 1);
        assert_eq!(quintile(45), 4);
    }
}
