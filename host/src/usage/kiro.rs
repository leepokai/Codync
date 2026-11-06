//! `kiro-cli` given `/usage` then `/quit` on stdin. Monthly plan credits and
//! bonus credits, as a share used.

use std::time::Duration;

use super::Window;

pub(super) async fn collect() -> Option<Vec<Window>> {
    let bin = super::command::program("kiro-cli")?;
    let text = super::command::output(&bin, &[], Some("/usage\n/quit\n"), Duration::from_secs(30)).await?;
    let windows = parse(&text, crate::store::now_ms());
    if windows.is_empty() {
        tracing::debug!("kiro-cli usage output had no credit lines");
        None
    } else {
        Some(windows)
    }
}

fn parse(text: &str, now_ms: i64) -> Vec<Window> {
    let text = super::command::strip_ansi(text);
    let mut windows = Vec::new();
    if let Some(w) = pair(&text, "Bonus credits:", "bonus", "Bonus credits", expiry_ms(&text, now_ms)) {
        windows.push(w);
    }
    if let Some(w) = credits(&text, now_ms) {
        windows.push(w);
    }
    windows
}

/// `Bonus credits: X/Y` — X is the amount already used.
fn pair(text: &str, marker: &str, id: &str, label: &str, resets: Option<i64>) -> Option<Window> {
    let at = text.find(marker)?;
    let rest = text[at + marker.len()..].trim_start();
    let (left, right) = rest.split_once('/')?;
    let used: f64 = take_number(left)?;
    let total: f64 = take_number(right)?;
    if total <= 0.0 {
        return None;
    }
    Some(Window::new(id, label, (used / total * 100.0).clamp(0.0, 100.0), resets))
}

fn credits(text: &str, now_ms: i64) -> Option<Window> {
    let at = text.find("Credits (")?;
    let rest = &text[at + "Credits (".len()..];
    let (used_text, after) = rest.split_once(" of ")?;
    let used: f64 = take_number(used_text)?;
    let total: f64 = take_number(after)?;
    if total <= 0.0 {
        return None;
    }
    Some(Window::new("monthly", "Monthly", (used / total * 100.0).clamp(0.0, 100.0), reset_on(text, now_ms)))
}

fn expiry_ms(text: &str, now_ms: i64) -> Option<i64> {
    let at = text.find("expires in ")?;
    let days: i64 = take_number(&text[at + "expires in ".len()..])?;
    Some(now_ms.saturating_add(days.saturating_mul(86_400_000)))
}

/// `resets on MM/DD`: this year's date, or next year's once that midnight has passed.
fn reset_on(text: &str, now_ms: i64) -> Option<i64> {
    let at = text.find("resets on ")?;
    let rest = text[at + "resets on ".len()..].trim_start();
    let (month, day) = rest.split_once('/')?;
    let month: u32 = month.trim().parse().ok()?;
    let day: u32 = take_number(day)?;
    let now = chrono::DateTime::from_timestamp_millis(now_ms)?.with_timezone(&chrono::Local);
    next_month_day(month, day, now)
}

fn next_month_day(month: u32, day: u32, now: chrono::DateTime<chrono::Local>) -> Option<i64> {
    use chrono::{Datelike, TimeZone};
    let year = now.year();
    let at = |y: i32| chrono::Local.with_ymd_and_hms(y, month, day, 0, 0, 0).single();
    let date = at(year).filter(|d| *d >= now).or_else(|| at(year + 1))?;
    Some(date.timestamp_millis())
}

fn take_number<T: std::str::FromStr>(text: &str) -> Option<T> {
    let token: String = text.trim_start().chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    token.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monthly_credits_and_bonus_are_shares_used() {
        let text =
            "Bonus credits: 20/100 left, expires in 12 days\nCredits (25 of 200 covered in plan) resets on 11/02\n";
        let w = parse(text, 1_761_400_000_000);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].label, "Bonus credits");
        assert!((w[0].percent - 20.0).abs() < 1e-9);
        assert_eq!(w[0].resets_at, Some(1_761_400_000_000 + 12 * 86_400_000));
        assert_eq!(w[1].label, "Monthly");
        assert!((w[1].percent - 12.5).abs() < 1e-9);
        assert!(w[1].resets_at.is_some());
    }
}
