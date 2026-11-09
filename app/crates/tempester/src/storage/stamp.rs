//! Date stamps for backup folders, in UTC (no time-zone database without a dependency): `2026-11-07_213015Z`.

use std::time::{SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: i64 = 86_400;

/// The stamp of `seconds` since the Unix epoch.
pub fn utc_stamp(seconds: i64) -> String {
    let (year, month, day) = civil_from_days(seconds.div_euclid(SECONDS_PER_DAY));
    let in_day = seconds.rem_euclid(SECONDS_PER_DAY);
    format!(
        "{year:04}-{month:02}-{day:02}_{:02}{:02}{:02}Z",
        in_day / 3600,
        in_day % 3600 / 60,
        in_day % 60
    )
}

/// The stamp of now.
pub fn now_stamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
    utc_stamp(seconds)
}

/// Proleptic Gregorian date of a day count since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_utc_stamp_when_epoch_returns_1970() {
        assert_eq!(utc_stamp(0), "1970-01-01_000000Z");
    }

    #[test]
    fn test_utc_stamp_when_known_instants_returns_their_dates() {
        assert_eq!(utc_stamp(1_000_000_000), "2001-09-09_014640Z");
        assert_eq!(utc_stamp(951_782_400), "2000-02-29_000000Z");
        assert_eq!(utc_stamp(951_868_799), "2000-02-29_235959Z");
        assert_eq!(utc_stamp(951_868_800), "2000-03-01_000000Z");
    }

    #[test]
    fn test_utc_stamp_when_century_non_leap_year_returns_march_first() {
        // 2100 is not a leap year: 2100-02-28 is followed by 2100-03-01.
        assert_eq!(utc_stamp(4_107_542_400), "2100-03-01_000000Z");
    }

    #[test]
    fn test_utc_stamp_when_before_epoch_returns_1969() {
        assert_eq!(utc_stamp(-1), "1969-12-31_235959Z");
    }
}
