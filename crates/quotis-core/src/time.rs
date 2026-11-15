use std::time::{SystemTime, UNIX_EPOCH};

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// Parses an RFC 3339 / ISO 8601 timestamp to unix seconds, honouring the UTC offset.
/// A timestamp with no offset is read as UTC. Returns None for anything malformed.
pub fn parse_ts(s: &str) -> Option<i64> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 19 || !s.is_char_boundary(19) || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| {
        let part = s.get(r)?;
        part.bytes().all(|c| c.is_ascii_digit()).then(|| part.parse::<i64>().ok())?
    };
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    let (hh, mm, ss) = (n(11..13)?, n(14..16)?, n(17..19)?);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let days = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => 28 + leap as i64,
        _ => return None,
    };
    if d < 1 || d > days || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }

    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix(['.', ',']) {
        let digits = frac.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        rest = &frac[digits..];
    }
    let offset = match rest {
        "" | "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes()[0] {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            // Exactly +HH, +HHMM or +HH:MM. Anything else is not a timestamp.
            let o = &rest.as_bytes()[1..];
            let digits: String = match o.len() {
                2 | 4 => rest[1..].to_string(),
                5 if o[2] == b':' => format!("{}{}", &rest[1..3], &rest[4..]),
                _ => return None,
            };
            if !digits.bytes().all(|c| c.is_ascii_digit()) {
                return None;
            }
            let oh: i64 = digits[..2].parse().ok()?;
            let om: i64 = if digits.len() == 4 { digits[2..].parse().ok()? } else { 0 };
            if oh > 23 || om > 59 {
                return None;
            }
            sign * (oh * 3600 + om * 60)
        }
    };

    // Days since 1970-01-01 (Howard Hinnant's civil-date algorithm).
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + hh * 3600 + mm * 60 + ss - offset)
}

#[cfg(test)]
mod tests {
    use super::parse_ts;

    #[test]
    fn utc_forms() {
        assert_eq!(parse_ts("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_ts("2000-03-01T00:00:00.123Z"), Some(951_868_800));
        assert_eq!(parse_ts("2026-09-22T09:38:12.943648+00:00"), Some(1_790_069_892));
        assert_eq!(parse_ts("2026-09-22 09:38:12"), Some(1_790_069_892));
        assert_eq!(parse_ts(" 2026-09-22t09:38:12z "), Some(1_790_069_892));
    }

    #[test]
    fn nonzero_offsets_shift_the_instant() {
        let utc = parse_ts("2026-09-22T09:38:12Z").unwrap();
        assert_eq!(parse_ts("2026-09-22T15:08:12+05:30"), Some(utc));
        assert_eq!(parse_ts("2026-09-22T02:38:12-07:00"), Some(utc));
        assert_eq!(parse_ts("2026-09-22T02:38:12.5-0700"), Some(utc));
        assert_eq!(parse_ts("2026-09-22T11:38:12+02"), Some(utc));
        // The offset can move the instant across a day and year boundary.
        assert_eq!(parse_ts("2027-01-01T00:30:00+01:00"), parse_ts("2026-12-31T23:30:00Z"));
    }

    #[test]
    fn daylight_saving_transitions_are_plain_offsets() {
        // US spring forward 2026-03-08: 01:59:59 EST is one second before 03:00:00 EDT.
        let before = parse_ts("2026-03-08T01:59:59-05:00").unwrap();
        let after = parse_ts("2026-03-08T03:00:00-04:00").unwrap();
        assert_eq!(after - before, 1);
        // EU fall back 2026-10-25: 02:30 happens twice, one hour apart.
        let first = parse_ts("2026-10-25T02:30:00+02:00").unwrap();
        let second = parse_ts("2026-10-25T02:30:00+01:00").unwrap();
        assert_eq!(second - first, 3600);
    }

    #[test]
    fn leap_days() {
        assert_eq!(parse_ts("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(parse_ts("2026-02-29T00:00:00Z"), None);
        assert_eq!(parse_ts("1900-02-29T00:00:00Z"), None);
    }

    #[test]
    fn malformed_is_none() {
        for bad in [
            "",
            "garbage",
            "2026-09-22",
            "2026-13-01T00:00:00Z",
            "2026-00-10T00:00:00Z",
            "2026-04-31T00:00:00Z",
            "2026-09-22T24:00:00Z",
            "2026-09-22T09:60:00Z",
            "2026-09-22T09:38:12+5",
            "2026-09-22T09:38:12+25:00",
            "2026-09-22T09:38:12.Z",
            "2026-09-22T09:38:12 UTC",
            "2026-09-22T09:38:1ZZ",
            "+026-09-22T09:38:12Z",
            "2026-09-22T09:38:12+05:3x",
            "2026-09-22T09:38:12+0:8",
            "2026-09-22T09:38:12+08::00",
            "2026-09-22T09:38:12+:0800",
            "2026-09-22T09:38:12+080",
            "２０２６-09-22T09:38:12Z",
            "2026-09-22T09:38:1é",
        ] {
            assert_eq!(parse_ts(bad), None, "{bad:?} should not parse");
        }
    }
}
