//! Consistent, human-readable dates for the desktop interface.
use gtk::glib::{DateTime, TimeZone};
thread_local! {
    // Retain the immutable timezone rules instead of reopening/parsing the
    // zoneinfo file for every visible log row. Relative days still use now().
    static TORONTO: TimeZone = TimeZone::new(Some("America/Toronto"));
}
fn zone() -> TimeZone {
    TORONTO.with(Clone::clone)
}
fn local(ts: i64) -> Option<DateTime> {
    DateTime::from_unix_utc(ts).ok()?.to_timezone(&zone()).ok()
}
fn format(d: &DateTime, pattern: &str) -> String {
    d.format(pattern)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "Unavailable".into())
}
fn clock(d: &DateTime) -> String {
    format(d, "%l:%M %p").trim().to_owned()
}
fn day(d: &DateTime) -> String {
    if let Ok(now) = DateTime::now(&zone()) {
        for (offset, title) in [(0, "Today"), (-1, "Yesterday"), (1, "Tomorrow")] {
            if let Ok(other) = now.add_days(offset) {
                if format(d, "%F") == format(&other, "%F") {
                    return title.into();
                }
            }
        }
        if d.year() == now.year() {
            return format(d, "%b %-d");
        }
    }
    format(d, "%b %-d, %Y")
}
pub fn day_key(ts: i64) -> Option<(i32, i32, i32)> {
    local(ts).map(|d| (d.year(), d.month(), d.day_of_month()))
}
pub fn when(ts: i64) -> String {
    local(ts)
        .map(|d| format!("{} · {}", day(&d), clock(&d)))
        .unwrap_or_else(|| "Unavailable".into())
}
pub fn exact(ts: i64) -> String {
    local(ts)
        .map(|d| {
            format!(
                "{} at {} {}",
                format(&d, "%A, %B %-d, %Y"),
                format(&d, "%l:%M:%S %p").trim(),
                format(&d, "%Z")
            )
        })
        .unwrap_or_else(|| "Unavailable".into())
}
pub fn date(raw: &str) -> String {
    if raw.is_empty() {
        return "No report yet".into();
    }
    let segments: Vec<_> = raw.split('-').collect();
    if segments.len() != 3
        || segments[0].len() != 4
        || segments[1].len() != 2
        || segments[2].len() != 2
        || segments
            .iter()
            .any(|s| !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return "Unavailable".into();
    }
    let parts: Vec<i32> = segments.iter().map(|s| s.parse().unwrap()).collect();
    DateTime::new(&zone(), parts[0], parts[1], parts[2], 12, 0, 0.)
        .map(|d| day(&d))
        .unwrap_or_else(|_| "Unavailable".into())
}
pub fn systemd(raw: &str) -> Option<i64> {
    raw.trim()
        .strip_prefix('@')?
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0.)
        .map(|n| n as i64)
}

pub fn source_date(raw: &str) -> String {
    let p: Vec<_> = raw.split_whitespace().collect();
    if p.len() != 5 {
        return raw.to_owned();
    }
    let t: Vec<_> = p[3].split(':').collect();
    let Some(hour) = t
        .first()
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|h| *h < 24)
    else {
        return raw.to_owned();
    };
    if t.len() < 2 {
        return raw.to_owned();
    }
    format!(
        "{} {}, {} · {}:{} {}",
        p[1],
        p[2].trim_start_matches('0'),
        p[4],
        if hour % 12 == 0 { 12 } else { hour % 12 },
        t[1],
        if hour < 12 { "AM" } else { "PM" }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scheduled_eight_am_uses_toronto_dst_and_readable_clock() {
        for (iso, suffix) in [
            ("2026-01-15T13:00:00Z", "8:00:00 AM EST"),
            ("2026-07-15T12:00:00Z", "8:00:00 AM EDT"),
        ] {
            let timestamp = DateTime::from_iso8601(iso, None).unwrap().to_unix();
            assert!(exact(timestamp).ends_with(suffix));
            assert!(when(timestamp).ends_with("8:00 AM"));
        }
    }
    #[test]
    fn row_dates_invalidate_at_toronto_midnight_in_both_seasons() {
        for (before, after, old, new) in [
            (
                "2026-10-28T03:59:59Z",
                "2026-10-28T04:00:00Z",
                (2026, 10, 27),
                (2026, 10, 28),
            ),
            (
                "2026-01-15T04:59:59Z",
                "2026-01-15T05:00:00Z",
                (2026, 1, 14),
                (2026, 1, 15),
            ),
        ] {
            assert_eq!(
                day_key(DateTime::from_iso8601(before, None).unwrap().to_unix()),
                Some(old)
            );
            assert_eq!(
                day_key(DateTime::from_iso8601(after, None).unwrap().to_unix()),
                Some(new)
            );
        }
    }
    #[test]
    fn invalid_systemd_timestamps_do_not_display_bad_dates() {
        for raw in ["n/a", "@NaN", "@inf", "@-1", ""] {
            assert_eq!(systemd(raw), None);
        }
        assert_eq!(date("bad"), "Unavailable");
        assert_eq!(date(""), "No report yet");
        assert_eq!(date("2026-x-10-26"), "Unavailable");
    }
}
