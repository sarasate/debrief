//! ISO-8601 UTC timestamps without a date crate. Transcripts and review
//! state both use `2026-09-27T19:26:25.459Z`.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn now_iso() -> String {
    iso_from_ms(now_ms())
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn iso_from_ms(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        ms.rem_euclid(1000)
    )
}

/// `2026-09-27T19:26:25.459Z` → seconds since the epoch. Only UTC with a
/// `Z` is accepted.
pub fn epoch_secs(ts: &str) -> Option<i64> {
    let (date, time) = ts.strip_suffix('Z')?.split_once('T')?;
    let mut d = date.splitn(3, '-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.splitn(3, ':');
    let (hh, mm) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let ss = t.next()?.split('.').next()?.parse::<i64>().ok()?;
    // Days from civil (Howard Hinnant's algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_secs_parses_transcript_timestamps() {
        assert_eq!(epoch_secs("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(epoch_secs("2026-09-27T19:26:25.459Z"), Some(1_790_537_185));
        assert_eq!(epoch_secs("2024-02-29T12:00:00Z"), Some(1_709_208_000));
        assert_eq!(epoch_secs("2026-09-27 19:26:25"), None);
    }

    #[test]
    fn iso_round_trips() {
        assert_eq!(iso_from_ms(1_790_537_185_459), "2026-09-27T19:26:25.459Z");
        assert_eq!(iso_from_ms(1_709_208_000_000), "2024-02-29T12:00:00.000Z");
        let now = now_ms();
        assert_eq!(epoch_secs(&iso_from_ms(now)), Some(now / 1000));
    }
}
