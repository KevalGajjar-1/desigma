//! RFC 3339 timestamps without a date-time dependency (UTC, millisecond precision).

use std::time::{SystemTime, UNIX_EPOCH};

/// Wall-clock milliseconds since the Unix epoch.
pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Date of a day count since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `2026-10-11T09:30:00.123Z`.
pub fn format_rfc3339(ms: u64) -> String {
    let secs = (ms / 1000) as i64;
    let (y, mo, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    format!("{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z", rem / 3600, rem % 3600 / 60, rem % 60, ms % 1000)
}

pub fn now_rfc3339() -> String {
    format_rfc3339(now_ms())
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fff](Z|±HH:MM)` into epoch milliseconds.
pub fn parse_rfc3339(s: &str) -> Option<u64> {
    let s = s.trim();
    let num = |t: &str| t.parse::<i64>().ok();
    let (date, rest) = s.split_once(['T', 't', ' '])?;
    let mut dp = date.split('-');
    let (y, mo, d) = (num(dp.next()?)?, num(dp.next()?)?, num(dp.next()?)?);
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return None;
    }
    let (time, offset_min) = if let Some(t) = rest.strip_suffix(['Z', 'z']) {
        (t, 0)
    } else {
        let at = rest.rfind(['+', '-'])?;
        let (t, off) = rest.split_at(at);
        let sign = if off.starts_with('-') { -1 } else { 1 };
        let (oh, om) = off[1..].split_once(':')?;
        (t, sign * (num(oh)? * 60 + num(om)?))
    };
    let (hms, frac) = time.split_once('.').unwrap_or((time, ""));
    let mut tp = hms.split(':');
    let (h, mi, sec) = (num(tp.next()?)?, num(tp.next()?)?, num(tp.next().unwrap_or("0"))?);
    if h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let millis: i64 = if frac.is_empty() { 0 } else { num(&format!("{:0<3}", &frac[..frac.len().min(3)]))? };
    let total = (days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec - offset_min * 60) * 1000 + millis;
    u64::try_from(total).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates_roundtrip() {
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_rfc3339(1_000_000_000_123), "2001-09-09T01:46:40.123Z");
        assert_eq!(parse_rfc3339("2001-09-09T01:46:40.123Z"), Some(1_000_000_000_123));
        assert_eq!(parse_rfc3339("2024-02-29T12:00:00Z"), Some(1_709_208_000_000), "leap day");
        for ms in [0u64, 86_399_999, 951_782_400_000, 4_102_444_800_000, 1_790_000_000_321] {
            assert_eq!(parse_rfc3339(&format_rfc3339(ms)), Some(ms));
        }
    }

    #[test]
    fn offsets_and_precision() {
        let utc = parse_rfc3339("2023-05-01T10:00:00Z").unwrap();
        assert_eq!(parse_rfc3339("2023-05-01T12:00:00+02:00"), Some(utc));
        assert_eq!(parse_rfc3339("2023-05-01T05:30:00-04:30"), Some(utc));
        assert_eq!(parse_rfc3339("2023-05-01T10:00:00.5Z"), Some(utc + 500));
        assert_eq!(parse_rfc3339("2023-05-01T10:00:00.123456Z"), Some(utc + 123));
    }

    #[test]
    fn garbage_is_rejected() {
        for bad in ["", "yesterday", "2023-13-01T00:00:00Z", "2023-05-01", "2023-05-01T25:00:00Z", "2023-05-01T10:00:00"] {
            assert_eq!(parse_rfc3339(bad), None, "{bad}");
        }
    }
}
