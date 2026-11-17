//! Times as the roster and chat print them, in local time.

use std::sync::LazyLock;

pub(super) fn now_ms() -> i64 {
    crate::store::now_ms()
}

/// Local UTC offset in seconds, read once.
fn utc_offset() -> i64 {
    static OFF: LazyLock<i64> = LazyLock::new(|| i64::from(chrono::Local::now().offset().local_minus_utc()));
    *OFF
}

/// (days since epoch, seconds into the local day)
pub(super) fn local(ms: i64) -> (i64, i64) {
    let s = ms.div_euclid(1000) + utc_offset();
    (s.div_euclid(86_400), s.rem_euclid(86_400))
}

pub(super) fn month_day(days: i64) -> (usize, i64) {
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (usize::try_from(m - 1).unwrap_or(0), d)
}

pub(super) const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
pub(super) const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];

pub fn clock(ms: i64) -> String {
    let (_, secs) = local(ms);
    format!("{:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

/// Roster time: 12:31 today, Tue this week, Sep 19 before.
pub fn when(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let (day, _) = local(ms);
    let (today, _) = local(now_ms());
    if now_ms() - ms < 60_000 {
        "now".into()
    } else if day == today {
        clock(ms)
    } else if today - day < 7 {
        DAYS[usize::try_from(day.rem_euclid(7)).unwrap_or(0)].into()
    } else {
        let (m, d) = month_day(day);
        format!("{} {d}", MONTHS[m])
    }
}

pub fn elapsed(start: Option<i64>) -> String {
    start.map_or_else(String::new, |start| span(start, now_ms()))
}

/// The time from `start` to `end` (epoch ms), as the working line counts it.
pub fn span(start: i64, end: i64) -> String {
    let s = ((end - start) / 1000).max(0);
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{}h{:02}m", s / 3600, s % 3600 / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(month_day(0), (0, 1)); // 1970-01-01
        assert_eq!(month_day(20_454), (0, 1)); // 2026-01-01
        assert_eq!(month_day(20_721), (8, 25)); // 2026-09-25
        assert_eq!(DAYS[usize::try_from(20_721_i64.rem_euclid(7)).unwrap_or(0)], "Fri");
    }
}
