//! When a routine runs, as typed in the terminal: `webhook`, `every 30m`, a Repeat preset
//! (`daily 09:00`), a date and time, or cron, each optionally followed by a time zone.

use crate::routines::ScheduleDraft;
use serde_json::{Value, json};
use std::str::FromStr as _;

/// One trigger from what's typed. A trailing time zone (`daily 09:00 Europe/Paris`, any case)
/// replaces `zone` for clock times; one still being typed gets a hint at what Tab picks.
pub fn parse_when(text: &str, zone: &str, now: i64) -> Result<Value, String> {
    let (rest, last) = split_last(text);
    if let Some(z) = exact_zone(last) {
        return parse_in(rest, z, now);
    }
    parse_in(text, zone, now).map_err(|e| match zone_attempt(text, now) {
        Some((_, word)) => match best_zone(word) {
            Some(z) => format!("Unknown time zone; tab picks {z}."),
            None => "Unknown time zone.".into(),
        },
        None => e,
    })
}

/// `text` with its last word completed to the closest time zone, when it's a clock schedule
/// followed by part of one (`daily 09:00 taipei` → `daily 09:00 Asia/Taipei`).
pub fn complete_zone(text: &str, now: i64) -> Option<String> {
    let (rest, word) = zone_attempt(text, now)?;
    Some(format!("{} {}", rest.trim(), best_zone(word)?))
}

/// A routine's triggers as `parse_when` reads them, when they're one it can write; a cron
/// in another zone than `zone` ends with that zone.
pub fn when_text(triggers: &Value, zone: &str) -> Option<String> {
    let [t] = triggers.as_array()?.as_slice() else { return None };
    match t["type"].as_str()? {
        "webhook" => Some("webhook".into()),
        "interval" => {
            let secs = t["seconds"].as_i64()?;
            let (n, u) = [(86400, "d"), (3600, "h"), (60, "m"), (1, "s")].into_iter().find(|(u, _)| secs % u == 0)?;
            Some(format!("every {}{u}", secs / n))
        }
        "cron" => {
            let cron = t["expression"].as_str()?;
            let at = exact_zone(t["timeZone"].as_str()?)?;
            let text = preset_text(cron).unwrap_or_else(|| cron.to_owned());
            Some(if at == zone { text } else { format!("{text} {at}") })
        }
        "once" => {
            let tz = chrono_tz::Tz::from_str(zone).ok()?;
            let at = chrono::DateTime::from_timestamp_millis(t["at"].as_i64()?)?.with_timezone(&tz);
            Some(at.format("%Y-%m-%d %H:%M").to_string())
        }
        _ => None,
    }
}

/// `parse_when` without a trailing zone: everything in `zone`.
fn parse_in(text: &str, zone: &str, now: i64) -> Result<Value, String> {
    let t = text.trim().to_lowercase();
    if t.is_empty() {
        return Err("Say when it runs.".into());
    }
    if t == "webhook" {
        return Ok(json!({"type": "webhook"}));
    }
    let words: Vec<&str> = t.split_whitespace().collect();
    if let Some(cron) = preset_cron(&words) {
        return Ok(json!({"type": "cron", "expression": cron?, "timeZone": zone}));
    }
    if let Some(rest) = t.strip_prefix("every") {
        let rest = rest.trim();
        let split = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        let (n, unit) = rest.split_at(split);
        let n: i64 = if n.is_empty() { 1 } else { n.parse().map_err(|_| "Use a whole number, like every 30m.")? };
        let unit = match unit.trim().trim_end_matches('s') {
            "sec" | "second" => 1,
            "m" | "min" | "minute" => 60,
            "h" | "hr" | "hour" => 3600,
            "d" | "day" => 86400,
            "" if n > 0 && !rest.is_empty() => return Err("Add a unit: s, m, h or d.".into()),
            _ => return Err("Use s, m, h or d, like every 2h.".into()),
        };
        return Ok(json!({"type": "interval", "seconds": n * unit}));
    }
    if let Ok(at) = chrono::NaiveDateTime::parse_from_str(&t, "%Y-%m-%d %H:%M") {
        let tz = chrono_tz::Tz::from_str(zone).map_err(|_| "Unknown time zone")?;
        let at = at.and_local_timezone(tz).earliest().ok_or("That time doesn't exist here.")?.timestamp_millis();
        if at <= now {
            return Err("That time has passed.".into());
        }
        return Ok(json!({"type": "once", "at": at}));
    }
    if words.len() == 5 {
        return Ok(json!({"type": "cron", "expression": words.join(" "), "timeZone": zone}));
    }
    Err("Try daily 09:00, weekdays 09:00, every 2h, 0 9 * * 1-5 or webhook.".into())
}

/// Weekdays in cron order (0 = Sunday); the first three letters are the short name.
const DAYS: [&str; 7] = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

/// The cron the host's routine editor compiles for a Repeat preset (`hourly`, `hourly :30`,
/// `daily 09:00`, `weekdays 09:00`, `weekly mon 09:00`), or `None` when the words aren't one.
fn preset_cron(words: &[&str]) -> Option<Result<String, String>> {
    let (&style, rest) = words.split_first()?;
    let time = |example: &str| match rest {
        [at] => clock(at).ok_or_else(|| format!("Use a 24-hour time, like {example}.")),
        _ => Err(format!("Use a 24-hour time, like {example}.")),
    };
    let draft = |(hour, minute): (u8, u8), weekday: u8| ScheduleDraft {
        calendar_style: style.into(),
        hour,
        minute,
        weekday,
        ..ScheduleDraft::default()
    };
    let draft = match style {
        "hourly" => match rest {
            [] => Ok(draft((0, 0), 1)),
            [m] => m
                .strip_prefix(':')
                .and_then(|m| small(m, 59))
                .map(|m| draft((0, m), 1))
                .ok_or_else(|| "Use a minute, like hourly :30.".into()),
            _ => Err("Use a minute, like hourly :30.".into()),
        },
        "daily" | "weekdays" => time(&format!("{style} 09:00")).map(|at| draft(at, 1)),
        "weekly" => {
            let [day, at] = rest else { return Some(Err("Use a day and time, like weekly mon 09:00.".into())) };
            let Some(d) = (0..).zip(DAYS).find_map(|(d, full)| (day.len() >= 3 && full.starts_with(day)).then_some(d))
            else {
                return Some(Err("Use a day from sun to sat, like weekly mon 09:00.".into()));
            };
            clock(at).map(|at| draft(at, d)).ok_or_else(|| "Use a 24-hour time, like weekly mon 09:00.".into())
        }
        _ => return None,
    };
    Some(draft.and_then(|d| d.cron().map_err(|e| format!("{e:#}"))))
}

/// A cron the host's routine editor reads as a Repeat preset, in its words; other crons get `None`.
fn preset_text(expression: &str) -> Option<String> {
    let mut draft =
        ScheduleDraft { calendar_style: "custom".into(), expression: expression.into(), ..ScheduleDraft::default() };
    draft.read_cron();
    let (minute, at) = (draft.minute, format!("{:02}:{:02}", draft.hour, draft.minute));
    Some(match draft.calendar_style.as_str() {
        "hourly" if minute == 0 => "hourly".into(),
        "hourly" => format!("hourly :{minute:02}"),
        "daily" | "weekdays" => format!("{} {at}", draft.calendar_style),
        "weekly" => format!("weekly {} {at}", DAYS.get(usize::from(draft.weekday))?.get(..3)?),
        _ => return None,
    })
}

/// `H:MM` or `HH:MM` on a 24-hour clock.
fn clock(at: &str) -> Option<(u8, u8)> {
    let (h, m) = at.split_once(':')?;
    Some((small(h, 23)?, small(m, 59)?))
}

/// One or two digits, at most `max`.
fn small(digits: &str, max: u8) -> Option<u8> {
    let plain = (1..=2).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit());
    plain.then(|| digits.parse().ok()).flatten().filter(|n| *n <= max)
}

/// The text before its last word, and that word.
fn split_last(text: &str) -> (&str, &str) {
    let text = text.trim_end();
    text.rsplit_once(char::is_whitespace).unwrap_or(("", text))
}

/// The IANA name `word` spells, in any case.
fn exact_zone(word: &str) -> Option<&'static str> {
    chrono_tz::TZ_VARIANTS.iter().map(|z| z.name()).find(|name| name.eq_ignore_ascii_case(word))
}

/// A clock schedule followed by a word that may be part of a time zone name: the two parts.
fn zone_attempt(text: &str, now: i64) -> Option<(&str, &str)> {
    let (rest, word) = split_last(text);
    let clock_time = |t: Value| matches!(t["type"].as_str(), Some("cron" | "once"));
    (word.chars().any(char::is_alphabetic)
        && exact_zone(word).is_none()
        && parse_in(rest, "UTC", now).is_ok_and(clock_time))
    .then_some((rest, word))
}

/// The time zone whose name holds every part of `word` ("/", "_" and "-" read as spaces):
/// cities starting with it first, then the shortest name.
fn best_zone(word: &str) -> Option<&'static str> {
    let norm = |s: &str| s.to_lowercase().replace(['/', '_', '-'], " ");
    let query = norm(word);
    let parts: Vec<&str> = query.split_whitespace().collect();
    let first = parts.first()?;
    chrono_tz::TZ_VARIANTS
        .iter()
        .map(|z| z.name())
        .filter(|name| parts.iter().all(|p| norm(name).contains(p)))
        .min_by_key(|name| (!norm(name.rsplit('/').next().unwrap_or(name)).starts_with(first), name.len(), *name))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_790_470_800_000;
    const ZONE: &str = "Asia/Taipei";

    #[test]
    fn when_reads_what_it_writes() {
        let (now, zone) = (NOW, ZONE);
        assert_eq!(parse_when("webhook", zone, now), Ok(json!({"type": "webhook"})));
        assert_eq!(parse_when("every 30m", zone, now), Ok(json!({"type": "interval", "seconds": 1800})));
        assert_eq!(parse_when("Every 2 hours", zone, now), Ok(json!({"type": "interval", "seconds": 7200})));
        assert!(parse_when("every 5", zone, now).is_err());
        assert!(parse_when("2020-01-01 09:00", zone, now).is_err());
        assert!(parse_when("tomorrow", zone, now).is_err());
        for text in ["0 9 * * 1-5", "every 90m", "every 1d", "webhook", "2030-10-01 09:00", "0 9 * * * Europe/Paris"] {
            let t = parse_when(text, zone, now).expect(text);
            let back = when_text(&json!([t]), zone).expect(text);
            assert_eq!(parse_when(&back, zone, now), Ok(t), "{text}");
        }
        // Several triggers, or a zone chrono-tz doesn't know, stay as they were.
        assert_eq!(when_text(&json!([{"type": "webhook"}, {"type": "webhook"}]), zone), None);
        assert_eq!(
            when_text(&json!([{"type": "cron", "expression": "0 9 * * *", "timeZone": "Mars/Base"}]), zone),
            None
        );
    }

    #[test]
    fn presets_write_the_editors_cron_and_read_back() {
        let (now, zone) = (NOW, ZONE);
        // Typed, the cron the host's editor writes for that Repeat style, and how editing shows it.
        for (typed, cron, shown) in [
            ("hourly", "0 * * * *", "hourly"),
            ("hourly :30", "30 * * * *", "hourly :30"),
            ("Hourly  :05", "5 * * * *", "hourly :05"),
            ("daily 08:30", "30 8 * * *", "daily 08:30"),
            (" DAILY   9:00 ", "0 9 * * *", "daily 09:00"),
            ("weekdays 09:00", "0 9 * * 1-5", "weekdays 09:00"),
            ("weekdays 23:59", "59 23 * * 1-5", "weekdays 23:59"),
            ("weekly sun 00:00", "0 0 * * 0", "weekly sun 00:00"),
            ("weekly mon 09:00", "0 9 * * 1", "weekly mon 09:00"),
            ("weekly Friday 17:45", "45 17 * * 5", "weekly fri 17:45"),
            ("weekly sat 12:00", "0 12 * * 6", "weekly sat 12:00"),
        ] {
            let t = json!({"type": "cron", "expression": cron, "timeZone": zone});
            assert_eq!(parse_when(typed, zone, now), Ok(t.clone()), "{typed}");
            assert_eq!(when_text(&json!([t]), zone).as_deref(), Some(shown), "{typed}");
            assert_eq!(parse_when(shown, zone, now), Ok(t), "{shown}");
        }
        // A cron the host's editor reads as a preset shows as one, and saves as the cron it compiles.
        let padded = json!([{"type": "cron", "expression": "0 09 * * *", "timeZone": zone}]);
        assert_eq!(when_text(&padded, zone).as_deref(), Some("daily 09:00"));
        // Crons no preset writes stay raw, and save back unchanged.
        for cron in ["0 9 * * 1,3,5", "*/15 * * * *", "0 9 1 * *", "0 9 * * 7", "0 * * * 1-5"] {
            let t = json!({"type": "cron", "expression": cron, "timeZone": zone});
            assert_eq!(when_text(&json!([t]), zone).as_deref(), Some(cron));
            assert_eq!(parse_when(cron, zone, now), Ok(t), "{cron}");
        }
        for (typed, error) in [
            ("hourly 30", "Use a minute, like hourly :30."),
            ("hourly :60", "Use a minute, like hourly :30."),
            ("daily", "Use a 24-hour time, like daily 09:00."),
            ("daily 24:00", "Use a 24-hour time, like daily 09:00."),
            ("daily 9am", "Use a 24-hour time, like daily 09:00."),
            ("weekdays 09:60", "Use a 24-hour time, like weekdays 09:00."),
            ("weekdays 9:00 10:00", "Use a 24-hour time, like weekdays 09:00."),
            ("weekly 09:00", "Use a day and time, like weekly mon 09:00."),
            ("weekly mo 09:00", "Use a day from sun to sat, like weekly mon 09:00."),
            ("weekly funday 09:00", "Use a day from sun to sat, like weekly mon 09:00."),
            ("weekly tue 7:5x", "Use a 24-hour time, like weekly mon 09:00."),
        ] {
            assert_eq!(parse_when(typed, zone, now), Err(error.into()), "{typed}");
        }
    }

    #[test]
    fn a_trailing_zone_replaces_the_local_one() {
        let cron = |expression: &str, zone: &str| json!({"type": "cron", "expression": expression, "timeZone": zone});
        // Any case; the stored name is the canonical one, and editing shows it only when it isn't local.
        for (typed, t, shown) in [
            ("daily 09:00 europe/london", cron("0 9 * * *", "Europe/London"), "daily 09:00 Europe/London"),
            ("weekdays 9:00 UTC", cron("0 9 * * 1-5", "UTC"), "weekdays 09:00 UTC"),
            ("0 9 * * 1,3 America/New_York", cron("0 9 * * 1,3", "America/New_York"), "0 9 * * 1,3 America/New_York"),
            ("hourly :15 Asia/Taipei", cron("15 * * * *", ZONE), "hourly :15"),
        ] {
            assert_eq!(parse_when(typed, ZONE, NOW), Ok(t.clone()), "{typed}");
            assert_eq!(when_text(&json!([t]), ZONE).as_deref(), Some(shown), "{typed}");
        }
        // A date and time in another zone is that zone's moment.
        let paris = parse_when("2030-10-01 09:00 Europe/Paris", ZONE, NOW).expect("once in Paris");
        assert_eq!(paris["at"], 1_917_068_400_000_i64);
        // Tab completes a partly typed zone after a clock schedule, fuzzily and in any case.
        for (typed, done) in [
            ("daily 09:00 taipei", "daily 09:00 Asia/Taipei"),
            ("weekdays 08:30 new-york", "weekdays 08:30 America/New_York"),
            ("0 9 * * 1  LOND", "0 9 * * 1 Europe/London"),
            ("2030-10-01 09:00 tokyo", "2030-10-01 09:00 Asia/Tokyo"),
        ] {
            assert_eq!(complete_zone(typed, NOW).as_deref(), Some(done), "{typed}");
        }
        // Nothing to complete: no schedule before the word, a whole zone, a number, or no clock time.
        for typed in ["daily", "daily 09:00", "daily 09:00 UTC", "weekdays 9:00 10:00", "every 2h paris", "webhook x"] {
            assert_eq!(complete_zone(typed, NOW), None, "{typed}");
        }
        assert_eq!(
            parse_when("daily 09:00 taipei", ZONE, NOW),
            Err("Unknown time zone; tab picks Asia/Taipei.".into())
        );
        assert_eq!(parse_when("daily 09:00 qqq", ZONE, NOW), Err("Unknown time zone.".into()));
    }
}
