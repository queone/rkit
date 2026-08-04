//! Historical statistics: frequency tables, chi-squared uniformity tests,
//! birthday-paradox duplicate estimation, and repeat-probability
//! projection for `cash5`, ported from Go's `stats.go`.

use crate::cash5::dates::{self, CivilDate};
use crate::cash5::model::Draw;
use crate::cash5::strategy::{extract_primary_five, get_payout, group_thousands};
use crate::color::{BLUE7, ColorMode, GRAY5, GREEN3};
use std::collections::HashMap;
use std::io::{self, Write};

#[derive(Clone, Copy, Default)]
pub struct NumCount {
    pub num: i32,
    pub count: i64,
}

#[derive(Clone)]
pub struct PairCount {
    pub pair: String,
    pub count: i64,
}

/// Ranks `freq` by count descending, `num` ascending on ties (a
/// deterministic tie-break Go's own `map`-iteration-based
/// `findMostCommon`/`findLeastCommon` lack — reused here for both, see
/// [`find_most_common`]/[`find_least_common`]).
pub fn find_top_n(freq: &HashMap<i32, i64>, n: usize) -> Vec<NumCount> {
    let mut results: Vec<NumCount> = freq
        .iter()
        .map(|(&num, &count)| NumCount { num, count })
        .collect();
    results.sort_by(|a, b| b.count.cmp(&a.count).then(a.num.cmp(&b.num)));
    results.truncate(n);
    results
}

pub fn find_bottom_n(freq: &HashMap<i32, i64>, n: usize) -> Vec<NumCount> {
    let mut results: Vec<NumCount> = freq
        .iter()
        .map(|(&num, &count)| NumCount { num, count })
        .collect();
    results.sort_by(|a, b| a.count.cmp(&b.count).then(a.num.cmp(&b.num)));
    results.truncate(n);
    results
}

pub fn find_top_n_pairs(freq: &HashMap<String, i64>, n: usize) -> Vec<PairCount> {
    let mut results: Vec<PairCount> = freq
        .iter()
        .map(|(pair, &count)| PairCount {
            pair: pair.clone(),
            count,
        })
        .collect();
    results.sort_by(|a, b| b.count.cmp(&a.count).then(a.pair.cmp(&b.pair)));
    results.truncate(n);
    results
}

/// The single most-common number, tie-broken deterministically (lowest
/// number) unlike Go's map-order-dependent `findMostCommon`.
pub fn find_most_common(freq: &HashMap<i32, i64>) -> NumCount {
    find_top_n(freq, 1).into_iter().next().unwrap_or_default()
}

pub fn find_least_common(freq: &HashMap<i32, i64>) -> NumCount {
    find_bottom_n(freq, 1)
        .into_iter()
        .next()
        .unwrap_or_default()
}

/// Chi-squared statistic for uniformity of `freq` over numbers 1-45 against
/// `total_draws` balls drawn.
pub fn calculate_chi_squared(freq: &HashMap<i32, i64>, total_draws: i64) -> f64 {
    let expected = total_draws as f64 / 45.0;
    let mut chi_squared = 0.0;
    for n in 1..=45 {
        let observed = *freq.get(&n).unwrap_or(&0) as f64;
        let diff = observed - expected;
        chi_squared += (diff * diff) / expected;
    }
    chi_squared
}

/// Wilson-Hilferty approximation of the p=0.05 chi-squared critical value.
pub fn chi_squared_critical(df: i64) -> f64 {
    if df <= 0 {
        return 0.0;
    }
    let d = df as f64;
    let x = 1.0 - 2.0 / (9.0 * d) + 1.6449 * (2.0 / (9.0 * d)).sqrt();
    d * x * x * x
}

/// Birthday-paradox expected duplicate-pair count for `n` draws from a pool
/// of `total_combos` possible combinations.
pub fn birthday_expected_duplicates(n: i64, total_combos: i64) -> f64 {
    n as f64 * (n - 1) as f64 / (2.0 * total_combos as f64)
}

pub fn birthday_std_dev(expected: f64) -> f64 {
    if expected <= 0.0 {
        0.0
    } else {
        expected.sqrt()
    }
}

pub struct SimulationResults {
    pub prob_30_days: f64,
    pub prob_90_days: f64,
    pub prob_365_days: f64,
    pub prob_10_years: f64,
}

/// Closed-form (not Monte Carlo, despite the historical Go name) estimate
/// of the probability of drawing a previously seen combination within the
/// given horizons.
pub fn run_repeat_simulation(num_historical: i64, total_combos: i64) -> SimulationResults {
    let p = num_historical as f64 / total_combos as f64;
    SimulationResults {
        prob_30_days: 1.0 - (1.0 - p).powi(30),
        prob_90_days: 1.0 - (1.0 - p).powi(90),
        prob_365_days: 1.0 - (1.0 - p).powi(365),
        prob_10_years: 1.0 - (1.0 - p).powi(3650),
    }
}

pub fn format_number(n: i64) -> String {
    group_thousands(&n.to_string())
}

fn dedupe_sorted_by_time(draws: &[Draw]) -> Vec<Draw> {
    let mut sorted: Vec<&Draw> = draws.iter().collect();
    sorted.sort_by_key(|d| d.draw_time);
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for draw in sorted {
        if seen.insert(draw.id.clone()) {
            unique.push(draw.clone());
        }
    }
    unique
}

fn eastern_date(millis: i64) -> CivilDate {
    dates::eastern_civil_time(millis).date
}

/// Renders the full statistics report to `out`, matching Go's
/// `displayStatistics` section-for-section (see `stats.go`).
pub(crate) fn display_statistics<W: Write>(
    draws: &[Draw],
    color: ColorMode,
    out: &mut W,
) -> io::Result<()> {
    if draws.is_empty() {
        return writeln!(out, "No draws found");
    }
    let unique = dedupe_sorted_by_time(draws);

    writeln!(out, "=== NJ CASH 5 STATISTICS ===")?;
    writeln!(out)?;
    writeln!(
        out,
        "{}: {}",
        color.paint(BLUE7, "Total Drawings"),
        color.paint(GREEN3, &unique.len().to_string())
    )?;

    let earliest = eastern_date(unique[0].draw_time);
    let latest_millis = unique[unique.len() - 1].draw_time;
    let latest = eastern_date(latest_millis);
    writeln!(
        out,
        "{}: {}",
        color.paint(BLUE7, "Earliest Drawing"),
        color.paint(GREEN3, &dates::ymd(earliest))
    )?;
    writeln!(
        out,
        "{}: {}",
        color.paint(BLUE7, "Latest Drawing"),
        color.paint(GREEN3, &dates::ymd(latest))
    )?;

    let mut smallest_payout = i64::MAX;
    let mut smallest_draw: Option<&Draw> = None;
    let mut winners_count = 0i64;
    let mut winner_millis: Vec<i64> = Vec::new();
    let mut all_winners: Vec<(&Draw, i64)> = Vec::new();

    let mut first_num_freq = HashMap::new();
    let mut pos2_freq = HashMap::new();
    let mut middle_num_freq = HashMap::new();
    let mut pos4_freq = HashMap::new();
    let mut last_num_freq = HashMap::new();
    let mut overall_freq = HashMap::new();
    let mut pair_freq: HashMap<String, i64> = HashMap::new();

    let last_30_days = latest_millis - 30 * 86_400_000;
    let last_60_days = latest_millis - 60 * 86_400_000;
    let last_90_days = latest_millis - 90 * 86_400_000;
    let mut freq30 = HashMap::new();
    let mut freq60 = HashMap::new();
    let mut freq90 = HashMap::new();

    for draw in &unique {
        let payout = get_payout(draw);
        if payout > 0 {
            winners_count += 1;
            winner_millis.push(draw.draw_time);
            all_winners.push((draw, payout));
            if payout < smallest_payout {
                smallest_payout = payout;
                smallest_draw = Some(draw);
            }
        }

        if let Ok(nums) = extract_primary_five(draw) {
            *first_num_freq.entry(nums[0]).or_insert(0i64) += 1;
            *pos2_freq.entry(nums[1]).or_insert(0i64) += 1;
            *middle_num_freq.entry(nums[2]).or_insert(0i64) += 1;
            *pos4_freq.entry(nums[3]).or_insert(0i64) += 1;
            *last_num_freq.entry(nums[4]).or_insert(0i64) += 1;
            for &n in &nums {
                *overall_freq.entry(n).or_insert(0i64) += 1;
                if draw.draw_time > last_30_days {
                    *freq30.entry(n).or_insert(0i64) += 1;
                }
                if draw.draw_time > last_60_days {
                    *freq60.entry(n).or_insert(0i64) += 1;
                }
                if draw.draw_time > last_90_days {
                    *freq90.entry(n).or_insert(0i64) += 1;
                }
            }
            for j in 0..nums.len() - 1 {
                for k in j + 1..nums.len() {
                    let pair = format!("{:02}-{:02}", nums[j], nums[k]);
                    *pair_freq.entry(pair).or_insert(0i64) += 1;
                }
            }
        }
    }

    writeln!(
        out,
        "\n{}: {}",
        color.paint(BLUE7, "Winners (5/5 Match)"),
        color.paint(GREEN3, &winners_count.to_string())
    )?;

    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Duplicate Combination Check")
    )?;
    let mut combination_map: HashMap<String, Vec<(String, i64)>> = HashMap::new();
    for draw in &unique {
        if let Ok(mut nums) = extract_primary_five(draw) {
            nums.sort_unstable();
            let key = format!(
                "{:02}-{:02}-{:02}-{:02}-{:02}",
                nums[0], nums[1], nums[2], nums[3], nums[4]
            );
            let date = dates::ymd(eastern_date(draw.draw_time));
            combination_map
                .entry(key)
                .or_default()
                .push((date, draw.draw_time));
        }
    }
    let mut duplicates: Vec<(&String, &Vec<(String, i64)>)> = combination_map
        .iter()
        .filter(|(_, dates)| dates.len() > 1)
        .collect();
    duplicates.sort_by(|a, b| a.0.cmp(b.0));

    const TOTAL_COMBINATIONS: i64 = 1_221_759; // C(45,5)
    let expected_dups = birthday_expected_duplicates(unique.len() as i64, TOTAL_COMBINATIONS);
    let dup_std_dev = birthday_std_dev(expected_dups);
    let observed_dups = duplicates.len() as i64;

    writeln!(
        out,
        "  {}: {}  {}",
        color.paint(BLUE7, "Observed duplicates"),
        color.paint(GREEN3, &observed_dups.to_string()),
        color.paint(
            GRAY5,
            &format!("(expected ~{expected_dups:.1} ± {dup_std_dev:.1} from birthday paradox)")
        )
    )?;

    if observed_dups == 0 {
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Status"),
            color.paint(
                GREEN3,
                &format!(
                    "✓ No duplicates ({} unique combinations in {} draws)",
                    combination_map.len(),
                    unique.len()
                )
            )
        )?;
    } else {
        let z_score = if dup_std_dev > 0.0 {
            (observed_dups as f64 - expected_dups) / dup_std_dev
        } else {
            0.0
        };
        if z_score > 2.0 {
            writeln!(
                out,
                "  {}: {}",
                color.paint(BLUE7, "Status"),
                color.paint(
                    GRAY5,
                    &format!("⚠ {observed_dups} duplicates exceeds expectation (z={z_score:.1})")
                )
            )?;
        } else {
            writeln!(
                out,
                "  {}: {}",
                color.paint(BLUE7, "Status"),
                color.paint(
                    GREEN3,
                    &format!(
                        "✓ {observed_dups} duplicates is consistent with random chance (z={z_score:.1})"
                    )
                )
            )?;
        }

        writeln!(out, "\n  {}:", color.paint(BLUE7, "Duplicate Details"))?;
        for (combo, dates_list) in &duplicates {
            writeln!(
                out,
                "    {}: {}",
                color.paint(BLUE7, "Combination"),
                color.paint(GREEN3, combo)
            )?;
            let mut sorted_dates = (*dates_list).clone();
            sorted_dates.sort_by_key(|(_, millis)| *millis);
            for (i, (date, millis)) in sorted_dates.iter().enumerate() {
                if i > 0 {
                    let gap = (millis - sorted_dates[i - 1].1) / 86_400_000;
                    if gap <= 30 {
                        writeln!(
                            out,
                            "        - {}",
                            color.paint(
                                GRAY5,
                                &format!("{date}  ← {gap}-day gap, warrants scrutiny")
                            )
                        )?;
                        continue;
                    }
                }
                writeln!(out, "        - {}", color.paint(GREEN3, date))?;
            }
        }
    }

    if !all_winners.is_empty() {
        let mut sorted_winners = all_winners.clone();
        sorted_winners.sort_by_key(|entry| std::cmp::Reverse(entry.1));
        let top_n = sorted_winners.len().min(10);
        writeln!(out, "\n{}:", color.paint(BLUE7, "Biggest Prizes"))?;
        writeln!(
            out,
            "  {}  {}  {}",
            color.paint(BLUE7, &format!("{:<14}", "Numbers")),
            color.paint(BLUE7, &format!("{:<10}", "Date")),
            color.paint(BLUE7, "Prize")
        )?;
        for (draw, payout) in &sorted_winners[..top_n] {
            if let Ok(nums) = extract_primary_five(draw) {
                let num_str = format!(
                    "{:02}-{:02}-{:02}-{:02}-{:02}",
                    nums[0], nums[1], nums[2], nums[3], nums[4]
                );
                writeln!(
                    out,
                    "  {}  {}  {}",
                    color.paint(GREEN3, &num_str),
                    color.paint(GREEN3, &dates::ymd(eastern_date(draw.draw_time))),
                    color.paint(
                        GREEN3,
                        &crate::cash5::strategy::format_currency(payout / 100)
                    )
                )?;
            }
        }
    }

    if let Some(draw) = smallest_draw
        && smallest_payout < i64::MAX
        && let Ok(nums) = extract_primary_five(draw)
    {
        let num_str = format!(
            "{:02}-{:02}-{:02}-{:02}-{:02}",
            nums[0], nums[1], nums[2], nums[3], nums[4]
        );
        writeln!(
            out,
            "\n{}: {}",
            color.paint(BLUE7, "Smallest Prize"),
            color.paint(
                GREEN3,
                &crate::cash5::strategy::format_currency(smallest_payout / 100)
            )
        )?;
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Date"),
            color.paint(GREEN3, &dates::ymd(eastern_date(draw.draw_time)))
        )?;
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Numbers"),
            color.paint(GREEN3, &num_str)
        )?;
    }

    if winner_millis.len() > 1 {
        let mut days_between = Vec::new();
        let mut longest_streak = 0i64;
        for i in 1..winner_millis.len() {
            let days = (winner_millis[i] - winner_millis[i - 1]) / 86_400_000;
            days_between.push(days);
            longest_streak = longest_streak.max(days);
        }
        let avg_days = days_between.iter().sum::<i64>() as f64 / days_between.len() as f64;
        writeln!(out, "\n{}:", color.paint(BLUE7, "Jackpot Win Frequency"))?;
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Average days between"),
            color.paint(GREEN3, &format!("{avg_days:.1} days"))
        )?;
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Longest streak"),
            color.paint(GREEN3, &format!("{longest_streak} days"))
        )?;
        let days_since_win = (latest_millis - winner_millis[winner_millis.len() - 1]) / 86_400_000;
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Days since last win"),
            color.paint(GREEN3, &format!("{days_since_win} days"))
        )?;
    }

    writeln!(out, "\n{}:", color.paint(BLUE7, "Most Common by Position"))?;
    for (label, freq) in [
        ("First position", &first_num_freq),
        ("Second position", &pos2_freq),
        ("Third position", &middle_num_freq),
        ("Fourth position", &pos4_freq),
        ("Fifth position", &last_num_freq),
    ] {
        let top = find_most_common(freq);
        writeln!(
            out,
            "  {}: {}  {}",
            color.paint(BLUE7, label),
            color.paint(GREEN3, &format!("{:02}", top.num)),
            color.paint(GRAY5, &format!("(appeared {} times)", top.count))
        )?;
    }

    writeln!(out, "\n{}:", color.paint(BLUE7, "Least Common by Position"))?;
    for (label, freq) in [
        ("First position", &first_num_freq),
        ("Second position", &pos2_freq),
        ("Third position", &middle_num_freq),
        ("Fourth position", &pos4_freq),
        ("Fifth position", &last_num_freq),
    ] {
        let bottom = find_least_common(freq);
        writeln!(
            out,
            "  {}: {}  {}",
            color.paint(BLUE7, label),
            color.paint(GREEN3, &format!("{:02}", bottom.num)),
            color.paint(GRAY5, &format!("(appeared {} times)", bottom.count))
        )?;
    }

    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Most Frequently Drawn (All Positions)")
    )?;
    for (i, nc) in find_top_n(&overall_freq, 5).iter().enumerate() {
        writeln!(
            out,
            "  {}. {} {}:  {}",
            color.paint(GREEN3, &(i + 1).to_string()),
            color.paint(BLUE7, "Number"),
            color.paint(GREEN3, &format!("{:02}", nc.num)),
            color.paint(GREEN3, &format!("{} times", nc.count))
        )?;
    }

    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Least Frequently Drawn (All Positions)")
    )?;
    for (i, nc) in find_bottom_n(&overall_freq, 5).iter().enumerate() {
        writeln!(
            out,
            "  {}. {} {}:  {}",
            color.paint(GREEN3, &(i + 1).to_string()),
            color.paint(BLUE7, "Number"),
            color.paint(GREEN3, &format!("{:02}", nc.num)),
            color.paint(GREEN3, &format!("{} times", nc.count))
        )?;
    }

    if !freq30.is_empty() {
        writeln!(
            out,
            "\n{}:",
            color.paint(BLUE7, "Hot Numbers (Last 30 Days)")
        )?;
        for (i, nc) in find_top_n(&freq30, 5).iter().enumerate() {
            writeln!(
                out,
                "  {}. {} {}:  {}",
                color.paint(GREEN3, &(i + 1).to_string()),
                color.paint(BLUE7, "Number"),
                color.paint(GREEN3, &format!("{:02}", nc.num)),
                color.paint(GREEN3, &format!("{} times", nc.count))
            )?;
        }
    }

    if !freq90.is_empty() {
        writeln!(
            out,
            "\n{}:",
            color.paint(BLUE7, "Cold Numbers (Last 90 Days)")
        )?;
        for (i, nc) in find_bottom_n(&freq90, 5).iter().enumerate() {
            writeln!(
                out,
                "  {}. {} {}:  {}",
                color.paint(GREEN3, &(i + 1).to_string()),
                color.paint(BLUE7, "Number"),
                color.paint(GREEN3, &format!("{:02}", nc.num)),
                color.paint(GREEN3, &format!("{} times", nc.count))
            )?;
        }
    }

    writeln!(out, "\n{}:", color.paint(BLUE7, "Most Common Number Pairs"))?;
    for (i, pc) in find_top_n_pairs(&pair_freq, 5).iter().enumerate() {
        writeln!(
            out,
            "  {}. {}:  {}",
            color.paint(GREEN3, &(i + 1).to_string()),
            color.paint(GREEN3, &pc.pair),
            color.paint(GREEN3, &format!("{} times", pc.count))
        )?;
    }

    let chi_squared = calculate_chi_squared(&overall_freq, unique.len() as i64 * 5);
    writeln!(out, "\n{}:", color.paint(BLUE7, "χ² Uniformity Analysis"))?;
    writeln!(
        out,
        "  {}: {}",
        color.paint(BLUE7, "χ² statistic"),
        color.paint(GREEN3, &format!("{chi_squared:.2}"))
    )?;
    writeln!(
        out,
        "  {}: {}",
        color.paint(BLUE7, "Degrees of freedom"),
        color.paint(GREEN3, "44 (45 numbers - 1)")
    )?;
    if chi_squared < 60.48 {
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Result"),
            color.paint(GREEN3, "Uniform distribution (p > 0.05)")
        )?;
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Interpretation"),
            color.paint(GRAY5, "Numbers appear randomly distributed")
        )?;
    } else if chi_squared < 66.77 {
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Result"),
            color.paint(GREEN3, "Possibly non-uniform (0.01 < p < 0.05)")
        )?;
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Interpretation"),
            color.paint(GRAY5, "Slight deviation from randomness")
        )?;
    } else {
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Result"),
            color.paint(GREEN3, "Non-uniform distribution (p < 0.01)")
        )?;
        writeln!(
            out,
            "  {}: {}",
            color.paint(BLUE7, "Interpretation"),
            color.paint(GRAY5, "Significant bias detected")
        )?;
    }

    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Full χ² Frequency Analysis Over History")
    )?;
    let mut pos2_chi = HashMap::new();
    let mut pos4_chi = HashMap::new();
    for draw in &unique {
        if let Ok(nums) = extract_primary_five(draw) {
            *pos2_chi.entry(nums[1]).or_insert(0i64) += 1;
            *pos4_chi.entry(nums[3]).or_insert(0i64) += 1;
        }
    }

    writeln!(
        out,
        "\n  {}:",
        color.paint(BLUE7, "Position-Specific Uniformity Tests")
    )?;
    let position_names = ["First", "Second", "Third", "Fourth", "Fifth"];
    let position_freqs = [
        &first_num_freq,
        &pos2_chi,
        &middle_num_freq,
        &pos4_chi,
        &last_num_freq,
    ];
    let mut all_positions_uniform = true;
    for (name, freq) in position_names.iter().zip(position_freqs.iter()) {
        let chi_sq = calculate_chi_squared(freq, unique.len() as i64);
        let is_uniform = chi_sq < 60.48;
        if !is_uniform {
            all_positions_uniform = false;
        }
        let status = if is_uniform {
            color.paint(GREEN3, "✓ Uniform")
        } else {
            color.paint(GRAY5, "⚠ Non-uniform")
        };
        writeln!(
            out,
            "    {} {}: χ²={}  {status}",
            color.paint(BLUE7, name),
            color.paint(BLUE7, "position"),
            color.paint(GREEN3, &format!("{chi_sq:.2}"))
        )?;
    }
    writeln!(
        out,
        "  {}: {}",
        color.paint(BLUE7, "Overall"),
        color.paint(
            GRAY5,
            if all_positions_uniform {
                "All positions show uniform distribution"
            } else {
                "Some positions show non-uniform distribution"
            }
        )
    )?;

    writeln!(
        out,
        "\n  {}:",
        color.paint(BLUE7, "Temporal Uniformity Analysis")
    )?;
    let mut yearly_freqs: HashMap<i64, HashMap<i32, i64>> = HashMap::new();
    let mut yearly_counts: HashMap<i64, i64> = HashMap::new();
    for draw in &unique {
        let year = eastern_date(draw.draw_time).year;
        *yearly_counts.entry(year).or_insert(0) += 1;
        if let Ok(nums) = extract_primary_five(draw) {
            let entry = yearly_freqs.entry(year).or_default();
            for n in nums {
                *entry.entry(n).or_insert(0) += 1;
            }
        }
    }
    let mut years: Vec<i64> = yearly_freqs.keys().copied().collect();
    years.sort_unstable();
    writeln!(out, "    {}:", color.paint(BLUE7, "Year-by-Year Analysis"))?;
    for year in years {
        let year_draws = *yearly_counts.get(&year).unwrap_or(&0);
        if year_draws >= 30 {
            let chi_sq = calculate_chi_squared(&yearly_freqs[&year], year_draws * 5);
            const DF: i64 = 44;
            let critical = chi_squared_critical(DF);
            let is_uniform = chi_sq < critical;
            let status = if is_uniform {
                color.paint(GREEN3, "✓")
            } else {
                color.paint(GRAY5, "⚠")
            };
            writeln!(
                out,
                "      {status} {}: χ²={}  {} draws  {}",
                color.paint(BLUE7, &year.to_string()),
                color.paint(GREEN3, &format!("{chi_sq:.2}")),
                color.paint(GREEN3, &year_draws.to_string()),
                color.paint(GRAY5, &format!("(df={DF}, critical={critical:.1})"))
            )?;
        }
    }

    writeln!(
        out,
        "\n  {}:",
        color.paint(BLUE7, "Sequential Pair Uniformity")
    )?;
    writeln!(
        out,
        "    {}: {}",
        color.paint(BLUE7, "Testing"),
        color.paint(GRAY5, "Whether consecutive numbers appear uniformly")
    )?;
    let mut consecutive_pairs = 0i64;
    let mut total_pairs = 0i64;
    let mut consec_pair_freq: HashMap<String, i64> = HashMap::new();
    for draw in &unique {
        if let Ok(nums) = extract_primary_five(draw) {
            for j in 0..nums.len() - 1 {
                total_pairs += 1;
                if nums[j + 1] == nums[j] + 1 {
                    consecutive_pairs += 1;
                    let key = format!("{:02}-{:02}", nums[j], nums[j + 1]);
                    *consec_pair_freq.entry(key).or_insert(0) += 1;
                }
            }
        }
    }
    let expected_consecutive_rate = 4.0 / 44.0;
    let actual_rate = consecutive_pairs as f64 / total_pairs as f64;
    writeln!(
        out,
        "    {}: {}  {}",
        color.paint(BLUE7, "Consecutive pairs found"),
        color.paint(GREEN3, &format!("{consecutive_pairs}/{total_pairs}")),
        color.paint(GRAY5, &format!("({:.2}%)", actual_rate * 100.0))
    )?;
    writeln!(
        out,
        "    {}: {}",
        color.paint(BLUE7, "Expected rate"),
        color.paint(
            GREEN3,
            &format!("{:.2}%", expected_consecutive_rate * 100.0)
        )
    )?;
    let deviation = ((actual_rate - expected_consecutive_rate) / expected_consecutive_rate) * 100.0;
    if (-10.0..10.0).contains(&deviation) {
        writeln!(
            out,
            "    {}: {}  {}",
            color.paint(BLUE7, "Assessment"),
            color.paint(GREEN3, "Within expected range"),
            color.paint(GRAY5, &format!("({deviation:.1}% deviation)"))
        )?;
    } else {
        writeln!(
            out,
            "    {}: {}  {}",
            color.paint(BLUE7, "Assessment"),
            color.paint(GRAY5, "Outside expected range"),
            color.paint(GRAY5, &format!("({deviation:.1}% deviation)"))
        )?;
    }

    writeln!(
        out,
        "\n    {}:",
        color.paint(BLUE7, "Top 10 Consecutive Pairs")
    )?;
    let expected_per_pair = total_pairs as f64 * expected_consecutive_rate / 44.0;
    for (i, pc) in find_top_n_pairs(&consec_pair_freq, 10).iter().enumerate() {
        writeln!(
            out,
            "      {:2}. {}:  {}  {}",
            i + 1,
            color.paint(GREEN3, &pc.pair),
            color.paint(GREEN3, &format!("{} times", pc.count)),
            color.paint(GRAY5, &format!("(expected ~{expected_per_pair:.1})"))
        )?;
    }

    let (mut low_consec, mut mid_consec, mut high_consec) = (0i64, 0i64, 0i64);
    for (pair, count) in &consec_pair_freq {
        let n1: i32 = pair.split('-').next().unwrap_or("0").parse().unwrap_or(0);
        match n1 {
            1..=15 => low_consec += count,
            16..=30 => mid_consec += count,
            _ => high_consec += count,
        }
    }
    writeln!(
        out,
        "\n    {}:",
        color.paint(BLUE7, "Consecutive Pairs by Range")
    )?;
    writeln!(
        out,
        "      {}: {}",
        color.paint(BLUE7, "Low (1-15)"),
        color.paint(GREEN3, &low_consec.to_string())
    )?;
    writeln!(
        out,
        "      {}: {}",
        color.paint(BLUE7, "Mid (16-30)"),
        color.paint(GREEN3, &mid_consec.to_string())
    )?;
    writeln!(
        out,
        "      {}: {}",
        color.paint(BLUE7, "High (31-44)"),
        color.paint(GREEN3, &high_consec.to_string())
    )?;

    writeln!(
        out,
        "\n  {}:",
        color.paint(BLUE7, "Low vs High Number Distribution")
    )?;
    writeln!(
        out,
        "    {}: {}",
        color.paint(BLUE7, "Testing"),
        color.paint(
            GRAY5,
            "Whether low (1-22) and high (23-45) numbers match hypergeometric expectation"
        )
    )?;
    let (mut low_count, mut high_count) = (0i64, 0i64);
    for draw in &unique {
        if let Ok(nums) = extract_primary_five(draw) {
            for n in nums {
                if n <= 22 {
                    low_count += 1;
                } else {
                    high_count += 1;
                }
            }
        }
    }
    let total_nums = low_count + high_count;
    let expected_low = total_nums as f64 * (22.0 / 45.0);
    let expected_high = total_nums as f64 * (23.0 / 45.0);
    let chi_sq_low_high = (low_count as f64 - expected_low).powi(2) / expected_low
        + (high_count as f64 - expected_high).powi(2) / expected_high;
    writeln!(
        out,
        "    {}: {}  {}",
        color.paint(BLUE7, "Low numbers (1-22)"),
        color.paint(GREEN3, &low_count.to_string()),
        color.paint(
            GRAY5,
            &format!("(hypergeometric expected: {expected_low:.0})")
        )
    )?;
    writeln!(
        out,
        "    {}: {}  {}",
        color.paint(BLUE7, "High numbers (23-45)"),
        color.paint(GREEN3, &high_count.to_string()),
        color.paint(
            GRAY5,
            &format!("(hypergeometric expected: {expected_high:.0})")
        )
    )?;
    writeln!(
        out,
        "    {}: {}  {}",
        color.paint(BLUE7, "χ² statistic"),
        color.paint(GREEN3, &format!("{chi_sq_low_high:.2}")),
        color.paint(GRAY5, "(df=1, critical=3.84 at p=0.05)")
    )?;
    if chi_sq_low_high < 3.84 {
        writeln!(
            out,
            "    {}: {}",
            color.paint(BLUE7, "Result"),
            color.paint(GREEN3, "✓ Balanced distribution")
        )?;
    } else {
        writeln!(
            out,
            "    {}: {}",
            color.paint(BLUE7, "Result"),
            color.paint(GRAY5, "⚠ Imbalanced — exceeds hypergeometric expectation")
        )?;
    }

    writeln!(out, "\n  {}:", color.paint(BLUE7, "Analysis Summary"))?;
    let mut issues_found = 0;
    if !all_positions_uniform {
        issues_found += 1;
    }
    if chi_sq_low_high >= 3.84 {
        issues_found += 1;
    }
    if issues_found == 0 {
        writeln!(
            out,
            "    {}",
            color.paint(
                GREEN3,
                "✓ All tests passed - lottery appears statistically fair"
            )
        )?;
    } else {
        writeln!(
            out,
            "    {}",
            color.paint(
                GRAY5,
                &format!("⚠ {issues_found} potential issues detected - review individual tests")
            )
        )?;
    }

    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Repeat Combination Analysis")
    )?;
    let num_historical = combination_map.len() as i64;
    let sim = run_repeat_simulation(num_historical, TOTAL_COMBINATIONS);
    writeln!(
        out,
        "  {}: {}",
        color.paint(BLUE7, "Historical combinations"),
        color.paint(GREEN3, &format!("{num_historical} unique sets"))
    )?;
    writeln!(
        out,
        "  {}: {}",
        color.paint(BLUE7, "Total possible combos"),
        color.paint(GREEN3, &format_number(TOTAL_COMBINATIONS))
    )?;
    writeln!(
        out,
        "  {}: {}",
        color.paint(BLUE7, "Coverage"),
        color.paint(
            GREEN3,
            &format!(
                "{:.4}%",
                num_historical as f64 * 100.0 / TOTAL_COMBINATIONS as f64
            )
        )
    )?;
    let bp_expected = birthday_expected_duplicates(unique.len() as i64, TOTAL_COMBINATIONS);
    let bp_std = birthday_std_dev(bp_expected);
    let bp_dev = if bp_std > 0.0 {
        (observed_dups as f64 - bp_expected) / bp_std
    } else {
        0.0
    };
    writeln!(
        out,
        "  {}: {}",
        color.paint(BLUE7, "Birthday paradox expected repeats"),
        color.paint(
            GREEN3,
            &format!(
                "~{bp_expected:.1} for {} draws from {} combos",
                unique.len(),
                format_number(TOTAL_COMBINATIONS)
            )
        )
    )?;
    writeln!(
        out,
        "  {}: {}",
        color.paint(BLUE7, "Observed repeats"),
        color.paint(
            GREEN3,
            &format!("{observed_dups} (z={bp_dev:.1}, consistent with expectation)")
        )
    )?;

    writeln!(
        out,
        "\n  {}:",
        color.paint(BLUE7, "Future repeat probability")
    )?;
    writeln!(
        out,
        "    {}: {}",
        color.paint(BLUE7, "In next 30 draws"),
        color.paint(GREEN3, &format!("{:.2}%", sim.prob_30_days * 100.0))
    )?;
    writeln!(
        out,
        "    {}: {}",
        color.paint(BLUE7, "In next 90 draws"),
        color.paint(GREEN3, &format!("{:.2}%", sim.prob_90_days * 100.0))
    )?;
    writeln!(
        out,
        "    {}: {}",
        color.paint(BLUE7, "In next 365 draws"),
        color.paint(GREEN3, &format!("{:.2}%", sim.prob_365_days * 100.0))
    )?;
    writeln!(
        out,
        "    {}: {}",
        color.paint(BLUE7, "In next 10 years"),
        color.paint(GREEN3, &format!("{:.2}%", sim.prob_10_years * 100.0))
    )?;

    writeln!(
        out,
        "\n{}:",
        color.paint(BLUE7, "Combinatorial Distance Scoring")
    )?;
    let coverage = num_historical as f64 * 100.0 / TOTAL_COMBINATIONS as f64;
    writeln!(
        out,
        "  {}",
        color.paint(
            GRAY5,
            &format!(
                "With {} draws covering {coverage:.2}% of {} possible combinations,",
                unique.len(),
                format_number(TOTAL_COMBINATIONS)
            )
        )
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(
            GRAY5,
            "the max-distance score is 2/5 for ~98% of all combos."
        )
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(
            GRAY5,
            "At current dataset density, distance-based selection provides no"
        )
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(
            GRAY5,
            "actionable signal — any random combination achieves the same score."
        )
    )?;
    writeln!(
        out,
        "  {}",
        color.paint(
            GRAY5,
            "(Skipping brute-force enumeration and simulated annealing.)"
        )
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_top_n_breaks_ties_by_ascending_num() {
        let mut freq = HashMap::new();
        freq.insert(5, 10);
        freq.insert(3, 10);
        freq.insert(7, 5);
        let top = find_top_n(&freq, 2);
        assert_eq!(top[0].num, 3);
        assert_eq!(top[1].num, 5);
    }

    #[test]
    fn find_most_common_and_least_common_are_deterministic() {
        let mut freq = HashMap::new();
        freq.insert(1, 3);
        freq.insert(2, 3);
        freq.insert(3, 1);
        assert_eq!(find_most_common(&freq).num, 1);
        assert_eq!(find_least_common(&freq).num, 3);
    }

    #[test]
    fn chi_squared_is_zero_for_perfectly_uniform_frequency() {
        let mut freq = HashMap::new();
        for n in 1..=45 {
            freq.insert(n, 10);
        }
        assert!(calculate_chi_squared(&freq, 450).abs() < 1e-9);
    }

    #[test]
    fn chi_squared_critical_matches_known_44df_value() {
        // p=0.05 critical value for df=44 is documented in Go as 60.48.
        assert!((chi_squared_critical(44) - 60.48).abs() < 0.1);
    }

    #[test]
    fn birthday_expected_duplicates_matches_formula() {
        assert_eq!(
            birthday_expected_duplicates(100, 1_000),
            100.0 * 99.0 / 2000.0
        );
        assert_eq!(birthday_std_dev(0.0), 0.0);
    }

    #[test]
    fn run_repeat_simulation_is_monotonic_in_horizon() {
        let sim = run_repeat_simulation(1000, 1_221_759);
        assert!(sim.prob_30_days < sim.prob_90_days);
        assert!(sim.prob_90_days < sim.prob_365_days);
        assert!(sim.prob_365_days < sim.prob_10_years);
    }

    #[test]
    fn format_number_groups_thousands() {
        assert_eq!(format_number(1_221_759), "1,221,759");
    }

    #[test]
    fn display_statistics_reports_no_draws_found() {
        let color = ColorMode::new(false);
        let mut out = Vec::new();
        display_statistics(&[], color, &mut out).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "No draws found\n");
    }
}
