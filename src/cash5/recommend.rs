//! Collision-avoiding number recommendation engine for `cash5`, ported
//! from the recommendation logic in Go's `main.go`.

use crate::cash5::model::Draw;
use crate::cash5::stats::{NumCount, find_bottom_n, find_top_n};
use crate::cash5::strategy::extract_primary_five;
use openssl::rand::rand_bytes;
use std::collections::{HashMap, HashSet};
use std::io::Write;

pub type WinnersSet = HashSet<[i32; 5]>;

/// Set of every historical winning combination (sorted 5-tuples). Draws
/// whose primary numbers can't be extracted are skipped.
pub fn build_winners_set(draws: &[Draw]) -> WinnersSet {
    let mut winners = HashSet::with_capacity(draws.len());
    for draw in draws {
        if let Ok(mut nums) = extract_primary_five(draw) {
            nums.sort_unstable();
            winners.insert(nums);
        }
    }
    winners
}

pub struct Recommendation {
    pub numbers: [i32; 5],
    pub strategy: &'static str,
}

pub const RECOMMENDATION_PREAMBLE: &str = "(none of these has previously won)";

/// Draws a uniformly distributed value in `0..bound` via rejection
/// sampling over `openssl::rand::rand_bytes` (already a pinned
/// dependency). Duplicated from `pgen.rs`'s equivalent (non-`pub`, and
/// `pgen.rs` isn't in this AC's file scope) rather than shared.
fn random_below(bound: u32) -> u32 {
    assert!(bound > 0, "bound must be positive");
    let limit = u32::MAX - (u32::MAX % bound);
    loop {
        let mut buffer = [0u8; 4];
        rand_bytes(&mut buffer).expect("system RNG unavailable");
        let value = u32::from_le_bytes(buffer);
        if value < limit {
            return value % bound;
        }
    }
}

/// Generates a random 5-number combination (1-45), sorted ascending.
pub fn generate_random_combo() -> [i32; 5] {
    let mut nums = [0i32; 5];
    let mut used = HashSet::new();
    for slot in nums.iter_mut() {
        loop {
            let n = random_below(45) as i32 + 1;
            if used.insert(n) {
                *slot = n;
                break;
            }
        }
    }
    nums.sort_unstable();
    nums
}

/// A random 5-number combo absent from `winners`. After a hard cap of
/// 1000 attempts (statistically unreachable) returns the final random
/// combo unconditionally.
pub fn generate_random_unwon_combo(winners: &WinnersSet) -> [i32; 5] {
    for _ in 0..1000 {
        let combo = generate_random_combo();
        if !winners.contains(&combo) {
            return combo;
        }
    }
    generate_random_combo()
}

fn count_consec_pairs(combo: &[i32; 5]) -> i32 {
    let mut count = 0;
    for i in 0..combo.len() - 1 {
        if combo[i + 1] == combo[i] + 1 {
            count += 1;
        }
    }
    count
}

/// The consecutive-pair-avoidance strategy with a winners filter: any
/// candidate already in `winners` is rejected outright.
pub fn generate_consec_avoid_combo_unique<E: Write>(
    winners: &WinnersSet,
    stderr: &mut E,
) -> [i32; 5] {
    let mut best: Option<[i32; 5]> = None;
    let mut best_consec = 0;
    for _ in 0..1000 {
        let combo = generate_random_combo();
        if winners.contains(&combo) {
            continue;
        }
        let consec = count_consec_pairs(&combo);
        if best.is_none() || consec < best_consec {
            best = Some(combo);
            best_consec = consec;
            if best_consec == 0 {
                break;
            }
        }
    }
    match best {
        Some(combo) => combo,
        None => {
            let _ = writeln!(
                stderr,
                "cash5: consec-avoid perturbation cap hit; falling back to random unwon combo"
            );
            generate_random_unwon_combo(winners)
        }
    }
}

/// Advances `idx` (a strictly ascending k-combination over `[0, k_total)`)
/// to the next combination in lex order. Returns `false` when `idx` is the
/// last combination.
pub fn next_lex_combo_indices(idx: &mut [usize], k_total: usize) -> bool {
    let k = idx.len();
    let mut i = k as isize - 1;
    while i >= 0 && idx[i as usize] == k_total - k + i as usize {
        i -= 1;
    }
    if i < 0 {
        return false;
    }
    idx[i as usize] += 1;
    for j in (i as usize + 1)..k {
        idx[j] = idx[j - 1] + 1;
    }
    true
}

/// Enumerates ascending 5-index subsets of the top-K ranked numbers in
/// lexicographic order and returns the first sorted combo absent from
/// `winners`. Falls back to a random unwon combo (with a stderr warning)
/// after `max_attempts` or after the rank space is exhausted.
pub fn first_unwon_from_top_k<E: Write>(
    ranks: &[NumCount],
    winners: &WinnersSet,
    max_attempts: usize,
    stderr: &mut E,
) -> [i32; 5] {
    if ranks.len() < 5 {
        return generate_random_unwon_combo(winners);
    }
    let k_total = ranks.len();
    let mut idx = [0usize, 1, 2, 3, 4];
    let mut attempts = 0usize;
    while attempts < max_attempts {
        let mut combo = [
            ranks[idx[0]].num,
            ranks[idx[1]].num,
            ranks[idx[2]].num,
            ranks[idx[3]].num,
            ranks[idx[4]].num,
        ];
        combo.sort_unstable();
        if !winners.contains(&combo) {
            return combo;
        }
        if !next_lex_combo_indices(&mut idx, k_total) {
            break;
        }
        attempts += 1;
    }
    let _ = writeln!(
        stderr,
        "cash5: top-K perturbation cap hit; falling back to random unwon combo"
    );
    generate_random_unwon_combo(winners)
}

/// Tries the natural pick (rank 0 from each position), then
/// deterministically swaps one slot at a time to its next-ranked
/// alternative until a sorted combo absent from `winners` is found or the
/// attempt cap is hit. Picks producing duplicate numbers across positions
/// are skipped.
pub fn first_unwon_by_position_swap<E: Write>(
    per_pos: &[Vec<NumCount>; 5],
    winners: &WinnersSet,
    max_attempts: usize,
    stderr: &mut E,
) -> [i32; 5] {
    let check = |idx: [usize; 5]| -> Option<[i32; 5]> {
        let mut combo = [0i32; 5];
        let mut seen = HashSet::new();
        for p in 0..5 {
            if idx[p] >= per_pos[p].len() {
                return None;
            }
            let v = per_pos[p][idx[p]].num;
            if !seen.insert(v) {
                return None;
            }
            combo[p] = v;
        }
        combo.sort_unstable();
        if winners.contains(&combo) {
            return None;
        }
        Some(combo)
    };

    if let Some(result) = check([0, 0, 0, 0, 0]) {
        return result;
    }

    let mut attempts = 0usize;
    let mut depth = 1usize;
    while attempts < max_attempts {
        let mut progressed = false;
        for slot in 0..5 {
            if depth >= per_pos[slot].len() {
                continue;
            }
            progressed = true;
            attempts += 1;
            let mut idx = [0usize; 5];
            idx[slot] = depth;
            if let Some(result) = check(idx) {
                return result;
            }
            if attempts >= max_attempts {
                break;
            }
        }
        if !progressed {
            break;
        }
        depth += 1;
    }
    let _ = writeln!(
        stderr,
        "cash5: position-swap perturbation cap hit; falling back to random unwon combo"
    );
    generate_random_unwon_combo(winners)
}

/// Creates 5 recommendations based on statistical analysis. Every
/// returned combination is absent from `winners`; on collision each
/// strategy performs a deterministic single-element swap to the
/// next-ranked alternative within its own ranking. Returns an empty `Vec`
/// when `unique_draws` is empty.
pub fn generate_recommendations<E: Write>(
    unique_draws: &[Draw],
    winners: &WinnersSet,
    stderr: &mut E,
) -> Vec<Recommendation> {
    let Some(latest_draw) = unique_draws.last() else {
        return Vec::new();
    };

    let mut overall_freq = HashMap::new();
    let mut first_num_freq = HashMap::new();
    let mut pos2_freq = HashMap::new();
    let mut middle_num_freq = HashMap::new();
    let mut pos4_freq = HashMap::new();
    let mut last_num_freq = HashMap::new();
    let mut freq30 = HashMap::new();

    let last_30_days = latest_draw.draw_time - 30 * 86_400_000;

    for draw in unique_draws {
        if let Ok(nums) = extract_primary_five(draw) {
            *first_num_freq.entry(nums[0]).or_insert(0i64) += 1;
            *pos2_freq.entry(nums[1]).or_insert(0i64) += 1;
            *middle_num_freq.entry(nums[2]).or_insert(0i64) += 1;
            *pos4_freq.entry(nums[3]).or_insert(0i64) += 1;
            *last_num_freq.entry(nums[4]).or_insert(0i64) += 1;
            for n in nums {
                *overall_freq.entry(n).or_insert(0i64) += 1;
                if draw.draw_time > last_30_days {
                    *freq30.entry(n).or_insert(0i64) += 1;
                }
            }
        }
    }

    let mut recs = Vec::new();

    let per_pos = [
        find_top_n(&first_num_freq, 10),
        find_top_n(&pos2_freq, 10),
        find_top_n(&middle_num_freq, 10),
        find_top_n(&pos4_freq, 10),
        find_top_n(&last_num_freq, 10),
    ];
    recs.push(Recommendation {
        numbers: first_unwon_by_position_swap(&per_pos, winners, 50, stderr),
        strategy: "Most common by position",
    });

    let top_overall = find_top_n(&overall_freq, 10);
    recs.push(Recommendation {
        numbers: first_unwon_from_top_k(&top_overall, winners, 50, stderr),
        strategy: "Most frequent",
    });

    let top_hot = find_top_n(&freq30, 10);
    recs.push(Recommendation {
        numbers: first_unwon_from_top_k(&top_hot, winners, 50, stderr),
        strategy: "Hot numbers last 30 days",
    });

    let least_per_pos = [
        find_bottom_n(&first_num_freq, 10),
        find_bottom_n(&pos2_freq, 10),
        find_bottom_n(&middle_num_freq, 10),
        find_bottom_n(&pos4_freq, 10),
        find_bottom_n(&last_num_freq, 10),
    ];
    recs.push(Recommendation {
        numbers: first_unwon_by_position_swap(&least_per_pos, winners, 50, stderr),
        strategy: "Least common by position",
    });

    recs.push(Recommendation {
        numbers: generate_consec_avoid_combo_unique(winners, stderr),
        strategy: "Consecutive pair avoidance",
    });

    recs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cash5::model::DrawResult;

    fn synth_draw(id: &str, draw_time: i64, nums: [i32; 5]) -> Draw {
        Draw {
            id: id.to_owned(),
            draw_time,
            results: vec![DrawResult {
                primary: nums.iter().map(|n| n.to_string()).collect(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn build_winners_set_collects_sorted_combos() {
        let draws = vec![
            synth_draw("d1", 0, [4, 20, 24, 26, 43]),
            synth_draw("d2", 86_400_000, [1, 2, 3, 4, 5]),
        ];
        let winners = build_winners_set(&draws);
        assert_eq!(winners.len(), 2);
        assert!(winners.contains(&[4, 20, 24, 26, 43]));
        assert!(winners.contains(&[1, 2, 3, 4, 5]));
    }

    fn synth_history(n: usize) -> Vec<Draw> {
        let base = 1_410_667_200_000i64;
        (0..n)
            .map(|i| {
                let mut combo = [
                    1 + i as i32 % 45,
                    1 + (i * 2 + 7) as i32 % 45,
                    1 + (i * 3 + 13) as i32 % 45,
                    1 + (i * 5 + 19) as i32 % 45,
                    1 + (i * 7 + 29) as i32 % 45,
                ];
                let mut seen = HashSet::new();
                for slot in combo.iter_mut() {
                    while seen.contains(slot) {
                        *slot = *slot % 45 + 1;
                    }
                    seen.insert(*slot);
                }
                synth_draw(&format!("d{i}"), base + i as i64 * 86_400_000, combo)
            })
            .collect()
    }

    #[test]
    fn generate_recommendations_avoids_historical_winners() {
        let draws = synth_history(60);
        let winners = build_winners_set(&draws);
        let mut stderr = Vec::new();
        let recs = generate_recommendations(&draws, &winners, &mut stderr);
        assert_eq!(recs.len(), 5);
        for rec in &recs {
            let mut sorted = rec.numbers;
            sorted.sort_unstable();
            assert!(
                !winners.contains(&sorted),
                "recommendation {:?} (strategy {}) is a historical winner",
                rec.numbers,
                rec.strategy
            );
            for n in rec.numbers {
                assert!((1..=45).contains(&n));
            }
            let unique: HashSet<i32> = rec.numbers.iter().copied().collect();
            assert_eq!(unique.len(), 5, "duplicate number in {:?}", rec.numbers);
        }
    }

    #[test]
    fn first_unwon_from_top_k_swaps_on_collision() {
        let ranks = [
            NumCount { num: 5, count: 100 },
            NumCount { num: 10, count: 90 },
            NumCount { num: 15, count: 80 },
            NumCount { num: 20, count: 70 },
            NumCount { num: 25, count: 60 },
            NumCount { num: 30, count: 50 },
            NumCount { num: 35, count: 40 },
            NumCount { num: 40, count: 30 },
            NumCount { num: 1, count: 20 },
            NumCount { num: 2, count: 10 },
        ];
        let mut winners = HashSet::new();
        winners.insert([5, 10, 15, 20, 25]);
        let mut stderr = Vec::new();
        let combo = first_unwon_from_top_k(&ranks, &winners, 50, &mut stderr);
        assert!(!winners.contains(&combo));
        assert_ne!(combo, [5, 10, 15, 20, 25]);
    }

    #[test]
    fn first_unwon_by_position_swap_swaps_on_collision() {
        let per_pos = [
            vec![
                NumCount { num: 1, count: 10 },
                NumCount { num: 6, count: 5 },
            ],
            vec![
                NumCount { num: 12, count: 10 },
                NumCount { num: 14, count: 5 },
            ],
            vec![
                NumCount { num: 20, count: 10 },
                NumCount { num: 22, count: 5 },
            ],
            vec![
                NumCount { num: 30, count: 10 },
                NumCount { num: 31, count: 5 },
            ],
            vec![
                NumCount { num: 40, count: 10 },
                NumCount { num: 41, count: 5 },
            ],
        ];
        let mut winners = HashSet::new();
        winners.insert([1, 12, 20, 30, 40]);
        let mut stderr = Vec::new();
        let combo = first_unwon_by_position_swap(&per_pos, &winners, 50, &mut stderr);
        assert!(!winners.contains(&combo));
        assert_ne!(combo, [1, 12, 20, 30, 40]);
    }

    #[test]
    fn next_lex_combo_indices_advances_and_terminates() {
        let mut idx = [0, 1, 2, 3, 4];
        assert!(next_lex_combo_indices(&mut idx, 10));
        assert_eq!(idx, [0, 1, 2, 3, 5]);

        let mut last = [5, 6, 7, 8, 9];
        assert!(!next_lex_combo_indices(&mut last, 10));
    }

    #[test]
    fn generate_random_combo_produces_five_distinct_in_range_numbers() {
        for _ in 0..20 {
            let combo = generate_random_combo();
            let unique: HashSet<i32> = combo.iter().copied().collect();
            assert_eq!(unique.len(), 5);
            for n in combo {
                assert!((1..=45).contains(&n));
            }
            assert!(combo.windows(2).all(|w| w[0] <= w[1]));
        }
    }
}
