//! UTC dates used by stored memories and transcript search.

pub fn ymd(ms: i64) -> String {
    if ms <= 0 {
        return "unknown date".into();
    }
    chrono::DateTime::from_timestamp_millis(ms)
        .map_or_else(|| "unknown date".into(), |date| date.format("%Y-%m-%d").to_string())
}

#[cfg(test)]
pub(super) fn parse_ymd(raw: &str) -> Option<i64> {
    Some(chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0)?.and_utc().timestamp_millis())
}
