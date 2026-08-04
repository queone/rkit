//! Draw-number extraction, match counting, and payout formatting for
//! `cash5`, ported from Go's `strategy.go`.

use crate::cash5::model::Draw;

/// Extracts and sorts-independent (caller sorts) the 5 primary numbers from
/// a draw's first result, matching Go's `extractPrimaryFive`. Parses each
/// entry's leading integer run, mirroring `fmt.Sscanf("%d")`.
pub fn extract_primary_five(draw: &Draw) -> Result<[i32; 5], String> {
    let result = draw.results.first().ok_or("no results in draw")?;
    if result.primary.len() < 5 {
        return Err("not enough primary numbers".to_owned());
    }
    let mut numbers = [0i32; 5];
    for (index, slot) in numbers.iter_mut().enumerate() {
        *slot = scan_leading_int(&result.primary[index])
            .ok_or_else(|| format!("cannot parse number {:?}", result.primary[index]))?;
    }
    Ok(numbers)
}

/// Parses the leading optionally-signed integer run in `s`, matching
/// `fmt.Sscanf("%d", ...)`'s tolerance of trailing non-digit characters.
fn scan_leading_int(s: &str) -> Option<i32> {
    let trimmed = s.trim_start();
    let mut end = 0;
    let bytes = trimmed.as_bytes();
    if end < bytes.len() && (bytes[end] == b'-' || bytes[end] == b'+') {
        end += 1;
    }
    let digits_start = end;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == digits_start {
        return None;
    }
    trimmed[..end].parse().ok()
}

/// Counts numbers two *sorted* slices have in common, matching Go's
/// `countMatches` merge-style comparison.
pub fn count_matches(a: &[i32], b: &[i32]) -> usize {
    let mut matches = 0;
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Equal => {
                matches += 1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    matches
}

/// Returns the actual 5/5 payout in cents, or 0 if there is no winner.
pub fn get_payout(draw: &Draw) -> i64 {
    if draw.actual_payout > 0 {
        return draw.actual_payout;
    }
    for tier in &draw.prize_tiers {
        let has_winners = tier.winners > 0 || tier.share_count > 0;
        let is_5_of_5 = tier.tier == "1"
            || tier.match_tier == "5"
            || tier.match_tier == "5/5"
            || tier.description == "5/5"
            || tier.name == "5/5"
            || tier.id == "1";
        if has_winners && is_5_of_5 {
            if tier.share_amount > 0 {
                return tier.share_amount;
            }
            if tier.prize_amount > 0 {
                return tier.prize_amount;
            }
        }
    }
    0
}

/// Formats a whole-dollar amount with thousand separators and a `$` prefix.
pub fn format_currency(amount: i64) -> String {
    let (sign, digits) = if amount < 0 {
        ("-", amount.unsigned_abs().to_string())
    } else {
        ("", amount.to_string())
    };
    format!("{sign}${}", group_thousands(&digits))
}

pub(crate) fn group_thousands(digits: &str) -> String {
    let bytes = digits.as_bytes();
    let mut result = Vec::with_capacity(bytes.len() + bytes.len() / 3);
    for (i, &byte) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            result.push(b',');
        }
        result.push(byte);
    }
    String::from_utf8(result).unwrap()
}

/// Formats `draw`'s 5/5 payout for display, `"$0"` when there is none.
pub fn format_payout(draw: &Draw) -> String {
    let payout = get_payout(draw);
    if payout > 0 {
        format_currency(payout / 100)
    } else {
        "$0".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cash5::model::{DrawResult, PrizeTier};

    fn draw_with_primary(nums: [&str; 5]) -> Draw {
        Draw {
            results: vec![DrawResult {
                primary: nums.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn extract_primary_five_parses_and_validates() {
        let draw = draw_with_primary(["4", "20", "24", "26", "43"]);
        assert_eq!(extract_primary_five(&draw).unwrap(), [4, 20, 24, 26, 43]);

        let empty = Draw::default();
        assert!(extract_primary_five(&empty).is_err());
    }

    #[test]
    fn count_matches_counts_sorted_overlap() {
        assert_eq!(count_matches(&[1, 5, 10, 20, 30], &[5, 15, 20, 25, 30]), 3);
        assert_eq!(count_matches(&[1, 2, 3], &[4, 5, 6]), 0);
    }

    #[test]
    fn get_payout_checks_actual_payout_then_prize_tiers() {
        let mut draw = Draw {
            actual_payout: 5_000_000,
            ..Default::default()
        };
        assert_eq!(get_payout(&draw), 5_000_000);

        draw.actual_payout = 0;
        draw.prize_tiers = vec![PrizeTier {
            tier: "1".into(),
            winners: 1,
            share_amount: 250_000,
            ..Default::default()
        }];
        assert_eq!(get_payout(&draw), 250_000);

        let no_winner = Draw {
            prize_tiers: vec![PrizeTier {
                tier: "1".into(),
                winners: 0,
                share_amount: 250_000,
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(get_payout(&no_winner), 0);
    }

    #[test]
    fn format_currency_groups_thousands() {
        assert_eq!(format_currency(0), "$0");
        assert_eq!(format_currency(999), "$999");
        assert_eq!(format_currency(1_000), "$1,000");
        assert_eq!(format_currency(1_234_567), "$1,234,567");
    }

    #[test]
    fn format_payout_falls_back_to_zero() {
        assert_eq!(format_payout(&Draw::default()), "$0");
        let draw = Draw {
            actual_payout: 25_000,
            ..Default::default()
        };
        assert_eq!(format_payout(&draw), "$250");
    }
}
