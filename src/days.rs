//! Calendar-day calculations and command-line behavior for `days`.

use crate::color::ColorMode;
use std::ffi::OsString;
use std::io::{self, Write};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const PROGRAM_NAME: &str = "days";
const WHITE: &str = "38;5;15";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CivilDate {
    year: i64,
    month: i64,
    day: i64,
}

impl CivilDate {
    fn parse(value: &str) -> Result<Self, String> {
        let pieces: Vec<&str> = value.split('-').collect();
        if pieces.len() != 3 || pieces[0].len() != 4 || pieces[2].len() != 2 {
            return Err("expected YYYY-MM-DD or YYYY-MMM-DD".to_owned());
        }
        let year = pieces[0]
            .parse::<i64>()
            .map_err(|_| "year is not numeric".to_owned())?;
        let month = if pieces[1].len() == 2 {
            pieces[1]
                .parse::<i64>()
                .map_err(|_| "month is not numeric".to_owned())?
        } else if pieces[1].len() == 3 {
            month_name(pieces[1]).ok_or_else(|| "month is not valid".to_owned())?
        } else {
            return Err("month must be numeric or a three-letter name".to_owned());
        };
        let day = pieces[2]
            .parse::<i64>()
            .map_err(|_| "day is not numeric".to_owned())?;
        let date = Self { year, month, day };
        if !date.is_valid() {
            return Err("date is outside the Gregorian calendar".to_owned());
        }
        Ok(date)
    }

    fn is_valid(self) -> bool {
        (1..=12).contains(&self.month)
            && (1..=days_in_month(self.year, self.month)).contains(&self.day)
    }

    fn add_days(self, days: i64) -> Self {
        civil_from_days(days_from_civil(self) + days)
    }

    fn format(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// Runs `days` and writes its process output to the supplied streams.
pub fn run<I, S, W, E>(args: I, version: &str, stdout: &mut W, stderr: &mut E) -> u8
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
    W: Write,
    E: Write,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    if args.iter().any(|arg| is_flag(arg, "-v", "--version")) && args.len() != 1 {
        let _ = writeln!(
            stderr,
            "days: version flag cannot be combined with other operands"
        );
        return 2;
    }
    if args.is_empty() || args.len() > 2 {
        let _ = write!(stdout, "{}", help(ColorMode::detect_stdout(), version));
        return 0;
    }
    if args.len() == 1 {
        if is_flag(&args[0], "-v", "--version") {
            let _ = writeln!(stdout, "{PROGRAM_NAME} v{version}");
            return 0;
        }
        if is_help_flag(&args[0]) {
            let _ = write!(stdout, "{}", help(ColorMode::detect_stdout(), version));
            return 0;
        }
        let value = args[0].to_string_lossy();
        if value.starts_with('+') || value.starts_with('-') {
            match value.parse::<i64>() {
                Ok(offset) => match local_today() {
                    Ok(today) => {
                        let _ = writeln!(stdout, "{}", today.add_days(offset).format());
                        return 0;
                    }
                    Err(error) => {
                        return write_error(stderr, format!("days: determine local date: {error}"));
                    }
                },
                Err(error) => {
                    return write_error(stderr, format!("days: bad offset {value:?}: {error}"));
                }
            }
        }
        if let Ok(offset) = value.parse::<i64>() {
            match local_today() {
                Ok(today) => {
                    let _ = writeln!(stdout, "{}", today.add_days(offset).format());
                    return 0;
                }
                Err(error) => {
                    return write_error(stderr, format!("days: determine local date: {error}"));
                }
            }
        }
        match CivilDate::parse(&value) {
            Ok(date) => match utc_today() {
                Ok(today) => {
                    let count = days_from_civil(date) - days_from_civil(today);
                    let _ = write!(stdout, "{}", print_days(count));
                    return 0;
                }
                Err(error) => {
                    return write_error(stderr, format!("days: determine UTC date: {error}"));
                }
            },
            Err(error) => return write_error(stderr, format!("days: bad date {value:?}: {error}")),
        }
    }

    let first = args[0].to_string_lossy();
    let second = args[1].to_string_lossy();
    let date1 = match CivilDate::parse(&first) {
        Ok(date) => date,
        Err(error) => return write_error(stderr, format!("days: bad date {first:?}: {error}")),
    };
    let date2 = match CivilDate::parse(&second) {
        Ok(date) => date,
        Err(error) => return write_error(stderr, format!("days: bad date {second:?}: {error}")),
    };
    let count = (days_from_civil(date1) - days_from_civil(date2)).abs();
    let _ = write!(stdout, "{}", print_days(count));
    0
}

fn write_error<E: Write>(stderr: &mut E, message: String) -> u8 {
    let _ = writeln!(stderr, "{message}");
    1
}

fn is_flag(value: &OsString, short: &str, long: &str) -> bool {
    value == short || value == long
}

fn is_help_flag(value: &OsString) -> bool {
    value == "-?" || value == "-h" || value == "--help"
}

fn help(color: ColorMode, version: &str) -> String {
    let name = color.paint(WHITE, PROGRAM_NAME);
    let overview = color.paint(WHITE, "Overview");
    format!(
        "{name} v{version}\n\
Calendar days calculator — https://github.com/queone/rkit\n\
{overview}\n\
  This utility works with calendar dates expressed as YYYY-MM-DD (or the equivalent\n\
  YYYY-MMM-DD format), and reports the relationship between today's date and the supplied\n\
  argument(s). Supported invocations are:\n\
\n\
    days -v, --version            Prints the version and exits.\n\
    days -N                       Prints the calendar date N days ago (e.g. -11).\n\
    days +N                       Prints the calendar date N days in the future (e.g. +6 or just 6).\n\
    days YYYY-MM-DD               Prints the number of days between today and the given date.\n\
    days YYYY-MM-DD YYYY-MM-DD    Prints the number of days between the two supplied dates.\n"
    )
}

fn month_name(value: &str) -> Option<i64> {
    match value.to_ascii_lowercase().as_str() {
        "jan" => Some(1),
        "feb" => Some(2),
        "mar" => Some(3),
        "apr" => Some(4),
        "may" => Some(5),
        "jun" => Some(6),
        "jul" => Some(7),
        "aug" => Some(8),
        "sep" => Some(9),
        "oct" => Some(10),
        "nov" => Some(11),
        "dec" => Some(12),
        _ => None,
    }
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn days_from_civil(date: CivilDate) -> i64 {
    let year = date.year - i64::from(date.month <= 2);
    let era = (if year >= 0 { year } else { year - 399 }) / 400;
    let year_of_era = year - era * 400;
    let month_prime = date.month + if date.month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + date.day - 1;
    let day_of_era = 365 * year_of_era + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> CivilDate {
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

fn print_days(days: i64) -> String {
    let original = days;
    let mut remaining = days.unsigned_abs();
    let mut years = 0_i64;
    while remaining >= 365 {
        let leap = i64::from(is_leap_year(years));
        if remaining < (365 + leap) as u64 {
            break;
        }
        remaining -= (365 + leap) as u64;
        years += 1;
    }
    if years > 0 {
        format!("{original} ({years} years + {remaining} days)\n")
    } else {
        format!("{original}\n")
    }
}

fn utc_today() -> io::Result<CivilDate> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::other(format!("system clock before Unix epoch: {error}")))?
        .as_secs();
    Ok(civil_from_days((seconds / 86_400) as i64))
}

fn local_today() -> io::Result<CivilDate> {
    if let Ok(output) = Command::new("date").arg("+%Y-%m-%d").output()
        && output.status.success()
    {
        let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if let Ok(date) = CivilDate::parse(&value) {
            return Ok(date);
        }
    }
    utc_today()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_case_insensitive_month_names() {
        assert_eq!(CivilDate::parse("2024-fEb-29").unwrap().month, 2);
        assert!(CivilDate::parse("2023-Feb-29").is_err());
    }

    #[test]
    fn civil_conversion_handles_leap_boundaries() {
        let feb = CivilDate::parse("2024-02-28").unwrap();
        assert_eq!(feb.add_days(2).format(), "2024-03-01");
        assert_eq!(
            days_from_civil(CivilDate::parse("2024-01-01").unwrap()),
            19_723
        );
    }

    #[test]
    fn print_days_preserves_upstream_breakdown() {
        assert_eq!(print_days(400), "400 (1 years + 34 days)\n");
        assert_eq!(print_days(-400), "-400 (1 years + 34 days)\n");
        assert_eq!(print_days(364), "364\n");
    }

    #[test]
    fn fixed_clock_calculations_are_deterministic() {
        let today = CivilDate::parse("2026-08-03").unwrap();
        assert_eq!(today.add_days(5).format(), "2026-08-08");
        let target = CivilDate::parse("2026-07-29").unwrap();
        assert_eq!(days_from_civil(target) - days_from_civil(today), -5);
    }
}
