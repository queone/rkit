//! Calendar-date math for `cash5`: UnixMilli <-> civil-date conversion,
//! narrative-date formatting, and the US Eastern-Time DST rule.
//!
//! Draw timestamps are stored as NJ-local midnight encoded as a UTC
//! instant (see `store.rs`). Go's runtime resolves these through the
//! *operator's OS-local timezone* (via IANA tzdata), which is only
//! equivalent to Eastern Time when the operator happens to run in
//! `America/New_York` — otherwise the same instant renders a different
//! calendar day depending on where the tool runs, an un-scrutinized Go
//! quirk with no test coverage. This port instead always renders draw
//! dates in Eastern Time, matching the data's own encoding and the
//! intended NJ-lottery use case (functional equivalence for the real
//! audience, not literal OS-local reproduction — consistent with this
//! repo's "match behavior, not implementation" dependency policy).
//! `needsRecentFetch` is the one piece of this behavior Go actually tests
//! against an explicit operator-local `now`, and is ported with an
//! explicit fixed UTC-offset parameter instead, decoupled from Eastern
//! Time entirely.
//!
//! The Hinnant civil<->days algorithm is duplicated from `days.rs` (out of
//! this AC's file scope, and its helpers aren't `pub`) rather than shared.

use std::io;
use std::process::Command;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CivilDate {
    pub year: i64,
    pub month: i64,
    pub day: i64,
}

pub fn days_from_civil(date: CivilDate) -> i64 {
    let year = date.year - i64::from(date.month <= 2);
    let era = (if year >= 0 { year } else { year - 399 }) / 400;
    let year_of_era = year - era * 400;
    let month_prime = date.month + if date.month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + date.day - 1;
    let day_of_era = 365 * year_of_era + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

pub fn civil_from_days(days: i64) -> CivilDate {
    let z = days + 719_468;
    let era = (if z >= 0 { z } else { z - 146_096 }) / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    CivilDate {
        year: year + i64::from(month <= 2),
        month,
        day,
    }
}

impl CivilDate {
    pub fn add_days(self, days: i64) -> Self {
        civil_from_days(days_from_civil(self) + days)
    }
}

/// 0=Sun .. 6=Sat. Epoch day 0 (1970-01-01) was a Thursday.
fn weekday_index(days_since_epoch: i64) -> i64 {
    (days_since_epoch % 7 + 7 + 4) % 7
}

const WEEKDAY_ABBREV: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH_ABBREV_LOWER: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

pub fn weekday_abbrev(days_since_epoch: i64) -> &'static str {
    WEEKDAY_ABBREV[weekday_index(days_since_epoch) as usize]
}

fn month_abbrev_lower(month: i64) -> &'static str {
    MONTH_ABBREV_LOWER[((month - 1).clamp(0, 11)) as usize]
}

/// Formats as `2026-feb-17`, matching Go's `narrativeDate`.
pub fn narrative_date(date: CivilDate) -> String {
    format!(
        "{}-{}-{:02}",
        date.year,
        month_abbrev_lower(date.month),
        date.day
    )
}

pub fn ymd(date: CivilDate) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

/// The day-of-month of the `n`th Sunday in `year`-`month` (1-based `n`).
fn nth_sunday(year: i64, month: i64, n: i64) -> i64 {
    let first_of_month = days_from_civil(CivilDate {
        year,
        month,
        day: 1,
    });
    let first_weekday = weekday_index(first_of_month);
    let first_sunday_day = 1 + (7 - first_weekday) % 7;
    first_sunday_day + 7 * (n - 1)
}

/// US Eastern-Time DST is in effect from the 2nd Sunday of March 02:00 to
/// the 1st Sunday of November 02:00 (the rule in force since 2007 — the
/// only rule ever relevant to cash5's post-2014-09-14 data range). The
/// 2:00am transition boundary is ignored: no cash5 timestamp falls near
/// it (draws anchor to midnight or 22:57 ET).
fn is_eastern_dst(date: CivilDate) -> bool {
    let dst_start = nth_sunday(date.year, 3, 2);
    let dst_end = nth_sunday(date.year, 11, 1);
    let day_of_year = days_from_civil(date);
    let start = days_from_civil(CivilDate {
        year: date.year,
        month: 3,
        day: dst_start,
    });
    let end = days_from_civil(CivilDate {
        year: date.year,
        month: 11,
        day: dst_end,
    });
    day_of_year >= start && day_of_year < end
}

fn eastern_offset_seconds(date: CivilDate) -> i64 {
    if is_eastern_dst(date) {
        -4 * 3600
    } else {
        -5 * 3600
    }
}

pub struct CivilTime {
    pub date: CivilDate,
    pub hour: i64,
    pub min: i64,
    pub sec: i64,
}

/// Decomposes `millis` (Unix epoch milliseconds) into civil date/time at a
/// fixed UTC offset (no DST logic — the offset is given, not derived).
pub fn civil_time_at_offset(millis: i64, offset_seconds: i64) -> CivilTime {
    let total_seconds = millis.div_euclid(1000) + offset_seconds;
    let days = total_seconds.div_euclid(86_400);
    let secs_of_day = total_seconds.rem_euclid(86_400);
    CivilTime {
        date: civil_from_days(days),
        hour: secs_of_day / 3600,
        min: (secs_of_day % 3600) / 60,
        sec: secs_of_day % 60,
    }
}

fn millis_from_civil_time_at_offset(time: &CivilTime, offset_seconds: i64) -> i64 {
    let days = days_from_civil(time.date);
    let seconds = days * 86_400 + time.hour * 3600 + time.min * 60 + time.sec - offset_seconds;
    seconds * 1000
}

/// Decomposes `millis` into Eastern-Time civil date/time, resolving DST via
/// a fixed-point iteration: an initial UTC-based guess determines a
/// candidate Eastern date, whose own DST status is then used for the final
/// conversion. Exact for any real cash5 timestamp (always anchored well
/// away from the 2am transition boundary).
pub fn eastern_civil_time(millis: i64) -> CivilTime {
    let guess = civil_time_at_offset(millis, eastern_offset_seconds(civil_from_days(0)));
    let offset = eastern_offset_seconds(guess.date);
    civil_time_at_offset(millis, offset)
}

/// Encodes a Y/M/D + H:M:S in Eastern Time as Unix epoch milliseconds,
/// matching Go's `time.ParseInLocation(..., easternTime())` for the backup
/// fetcher's scraped draw dates.
pub fn millis_from_eastern_civil(date: CivilDate, hour: i64, min: i64, sec: i64) -> i64 {
    let offset = eastern_offset_seconds(date);
    millis_from_civil_time_at_offset(
        &CivilTime {
            date,
            hour,
            min,
            sec,
        },
        offset,
    )
}

/// Resolves the operator's current instant and UTC offset via `date`
/// (portable across BSD and GNU `date`), matching `days.rs`'s
/// shell-out-for-local-time pattern (no IANA tzdata available otherwise).
pub fn local_now() -> io::Result<(i64, i64)> {
    let output = Command::new("date").arg("+%s %z").output()?;
    if !output.status.success() {
        return Err(io::Error::other("date command failed"));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parts = text.split_whitespace();
    let seconds: i64 = parts
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| io::Error::other("could not parse date output"))?;
    let offset_text = parts
        .next()
        .ok_or_else(|| io::Error::other("could not parse date offset"))?;
    let offset_seconds = parse_numeric_offset(offset_text)
        .ok_or_else(|| io::Error::other(format!("bad offset {offset_text:?}")))?;
    Ok((seconds * 1000, offset_seconds))
}

/// [`local_now`], falling back to UTC (offset 0) if the `date` shell-out
/// fails for any reason. Never errors, matching Go's infallible
/// `time.Now()`.
pub fn now_or_utc() -> (i64, i64) {
    local_now().unwrap_or_else(|_| {
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        (millis, 0)
    })
}

/// Parses a `+HHMM`/`-HHMM` numeric UTC offset into seconds.
fn parse_numeric_offset(text: &str) -> Option<i64> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, text.strip_prefix('+').unwrap_or(text)),
    };
    if digits.len() != 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let hours: i64 = digits[..2].parse().ok()?;
    let minutes: i64 = digits[2..].parse().ok()?;
    Some(sign * (hours * 3600 + minutes * 60))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn narrative_date_matches_go_format() {
        assert_eq!(
            narrative_date(CivilDate {
                year: 2026,
                month: 2,
                day: 17
            }),
            "2026-feb-17"
        );
    }

    #[test]
    fn weekday_abbrev_matches_known_anchor() {
        // 1970-01-01 (epoch day 0) was a Thursday.
        assert_eq!(weekday_abbrev(0), "Thu");
        // 2000-01-01 was a Saturday.
        let days = days_from_civil(CivilDate {
            year: 2000,
            month: 1,
            day: 1,
        });
        assert_eq!(weekday_abbrev(days), "Sat");
    }

    #[test]
    fn eastern_dst_matches_known_transition_dates() {
        // 2023: DST 2nd Sunday of March (Mar 12) to 1st Sunday of November (Nov 5).
        assert!(!is_eastern_dst(CivilDate {
            year: 2023,
            month: 3,
            day: 11
        }));
        assert!(is_eastern_dst(CivilDate {
            year: 2023,
            month: 3,
            day: 12
        }));
        assert!(is_eastern_dst(CivilDate {
            year: 2023,
            month: 11,
            day: 4
        }));
        assert!(!is_eastern_dst(CivilDate {
            year: 2023,
            month: 11,
            day: 5
        }));
        // 2025: March 9 to November 2.
        assert!(is_eastern_dst(CivilDate {
            year: 2025,
            month: 3,
            day: 9
        }));
        assert!(!is_eastern_dst(CivilDate {
            year: 2025,
            month: 11,
            day: 2
        }));
    }

    #[test]
    fn eastern_offset_encodes_midnight_correctly() {
        // NJ midnight EDT on 2026-05-13 is 2026-05-13 04:00 UTC.
        let millis = millis_from_eastern_civil(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            0,
            0,
            0,
        );
        let expected = days_from_civil(CivilDate {
            year: 2026,
            month: 5,
            day: 13,
        }) * 86_400_000
            + 4 * 3_600_000;
        assert_eq!(millis, expected);
    }

    #[test]
    fn eastern_civil_time_round_trips_through_encode() {
        let date = CivilDate {
            year: 2026,
            month: 5,
            day: 13,
        };
        let millis = millis_from_eastern_civil(date, 0, 0, 0);
        let decoded = eastern_civil_time(millis);
        assert_eq!(decoded.date, date);
        assert_eq!(decoded.hour, 0);

        // Also check a winter (EST) date for the other DST branch.
        let winter = CivilDate {
            year: 2026,
            month: 1,
            day: 15,
        };
        let winter_millis = millis_from_eastern_civil(winter, 22, 57, 0);
        let winter_decoded = eastern_civil_time(winter_millis);
        assert_eq!(winter_decoded.date, winter);
        assert_eq!(winter_decoded.hour, 22);
        assert_eq!(winter_decoded.min, 57);
    }

    #[test]
    fn civil_time_at_offset_honors_fixed_zones() {
        // EDT midnight on 2026-05-13 (04:00 UTC) read back at PDT (-7h)
        // is still 2026-05-12 local (21:00 the previous day) -- matching
        // needsRecentFetch's operator-local-TZ semantics.
        let millis = millis_from_eastern_civil(
            CivilDate {
                year: 2026,
                month: 5,
                day: 13,
            },
            0,
            0,
            0,
        );
        let pdt = civil_time_at_offset(millis, -7 * 3600);
        assert_eq!(pdt.date.day, 12);
        assert_eq!(pdt.hour, 21);
    }

    #[test]
    fn parse_numeric_offset_handles_sign_and_digits() {
        assert_eq!(parse_numeric_offset("-0400"), Some(-14_400));
        assert_eq!(parse_numeric_offset("+0200"), Some(7_200));
        assert_eq!(parse_numeric_offset("+0000"), Some(0));
        assert_eq!(parse_numeric_offset("bogus"), None);
    }
}
