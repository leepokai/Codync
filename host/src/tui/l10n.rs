//! This terminal's interface language. The phrase table is the one the phone apps ship.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use crate::LockExt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ChineseScript {
    Hans,
    Hant,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum AppLanguage {
    System,
    Zh,
    ZhHant,
    En,
    Ja,
    Ko,
    Fr,
    De,
    Es,
    PtBr,
    Ru,
    It,
}

impl AppLanguage {
    fn parse(raw: &str) -> Self {
        match raw {
            "zh" | "zh-Hans" => Self::Zh,
            "zh-Hant" => Self::ZhHant,
            "en" => Self::En,
            "ja" => Self::Ja,
            "ko" => Self::Ko,
            "fr" => Self::Fr,
            "de" => Self::De,
            "es" => Self::Es,
            "pt-BR" => Self::PtBr,
            "ru" => Self::Ru,
            "it" => Self::It,
            _ => Self::System,
        }
    }

    fn stored(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Zh => "zh",
            Self::ZhHant => "zh-Hant",
            Self::En => "en",
            Self::Ja => "ja",
            Self::Ko => "ko",
            Self::Fr => "fr",
            Self::De => "de",
            Self::Es => "es",
            Self::PtBr => "pt-BR",
            Self::Ru => "ru",
            Self::It => "it",
        }
    }

    fn next(self) -> Self {
        match self {
            Self::System => Self::Zh,
            Self::Zh => Self::ZhHant,
            Self::ZhHant => Self::En,
            Self::En => Self::Ja,
            Self::Ja => Self::Ko,
            Self::Ko => Self::Fr,
            Self::Fr => Self::De,
            Self::De => Self::Es,
            Self::Es => Self::PtBr,
            Self::PtBr => Self::Ru,
            Self::Ru => Self::It,
            Self::It => Self::System,
        }
    }
}

static HANS: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/zh-Hans.json"))
        .expect("zh-Hans.json is a string map")
});

static HANT: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/zh-Hant.json"))
        .expect("zh-Hant.json is a string map")
});

static JA: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/ja.json"))
        .expect("ja.json is a string map")
});

static KO: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/ko.json"))
        .expect("ko.json is a string map")
});

static FR: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/fr.json"))
        .expect("fr.json is a string map")
});

static DE: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/de.json"))
        .expect("de.json is a string map")
});

static ES: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/es.json"))
        .expect("es.json is a string map")
});

static PT_BR: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/pt-BR.json"))
        .expect("pt-BR.json is a string map")
});

static RU: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/ru.json"))
        .expect("ru.json is a string map")
});

static IT: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../apps/ios/Kit/Sources/CodyncKit/Resources/Languages/it.json"))
        .expect("it.json is a string map")
});

static PREFERENCE: LazyLock<Mutex<AppLanguage>> = LazyLock::new(|| Mutex::new(read_preference()));

const FILE: &str = "tui-language";

fn read_preference() -> AppLanguage {
    super::choice::read(FILE).map_or(AppLanguage::System, |text| AppLanguage::parse(text.trim()))
}

fn system_locale() -> Option<String> {
    std::env::var("LC_ALL").or_else(|_| std::env::var("LC_MESSAGES")).or_else(|_| std::env::var("LANG")).ok()
}

/// `zh-Hant`, `zh_TW`, `zh_HK` and `zh_MO` read Traditional. Other Chinese locales read Simplified.
fn system_script_of(value: &str) -> Option<ChineseScript> {
    let id = value.to_ascii_lowercase().split('.').next().unwrap_or("").replace('_', "-");
    if !id.starts_with("zh") {
        return None;
    }
    if id.contains("hant") || id.contains("-tw") || id.contains("-hk") || id.contains("-mo") {
        Some(ChineseScript::Hant)
    } else {
        Some(ChineseScript::Hans)
    }
}

/// The table for the saved choice. Traditional falls back to Simplified for a missing line.
fn resolved_language(preference: AppLanguage, locale: &str) -> AppLanguage {
    if preference != AppLanguage::System {
        return preference;
    }
    match system_script_of(locale) {
        Some(ChineseScript::Hans) => AppLanguage::Zh,
        Some(ChineseScript::Hant) => AppLanguage::ZhHant,
        None => {
            let locale = locale.to_ascii_lowercase();
            let language = locale.split(['-', '_', '.']).next().unwrap_or("en");
            match language {
                "pt" => AppLanguage::PtBr,
                _ => match AppLanguage::parse(language) {
                    AppLanguage::System => AppLanguage::En,
                    language => language,
                },
            }
        }
    }
}

fn active_phrases() -> Option<&'static HashMap<String, String>> {
    match resolved_language(*PREFERENCE.locked(), &system_locale().unwrap_or_default()) {
        AppLanguage::Zh => Some(&HANS),
        AppLanguage::ZhHant => Some(&HANT),
        AppLanguage::Ja => Some(&JA),
        AppLanguage::Ko => Some(&KO),
        AppLanguage::Fr => Some(&FR),
        AppLanguage::De => Some(&DE),
        AppLanguage::Es => Some(&ES),
        AppLanguage::PtBr => Some(&PT_BR),
        AppLanguage::Ru => Some(&RU),
        AppLanguage::It => Some(&IT),
        AppLanguage::En | AppLanguage::System => None,
    }
}

/// "resets 3h 20m" / "resets in 2d 6h". Compact times become "3小时20分后重置".
pub(in crate::tui) fn reset_line(raw: &str) -> String {
    let Some(time) = raw.strip_prefix("resets in ").or_else(|| raw.strip_prefix("resets ")) else {
        return tr(raw).into_owned();
    };
    let Some(table) = active_phrases() else { return raw.to_owned() };
    if !std::ptr::eq(table, &raw const *HANS) && !std::ptr::eq(table, &raw const *HANT) {
        return if compact_time_in(time, table).is_some() {
            translate_with("resets in {time}", table).replace("{time}", time)
        } else {
            format!("{} {time}", translate_with("resets", table))
        };
    }
    match compact_time_in(time, table) {
        Some(spoken) => format!("{spoken}{}", translate_with("after reset", table)),
        None => format!("{} {time}", translate_with("resets", table)),
    }
}

/// "3h 20m", "2d 6h", "45m", on the Simplified table. The live reset line uses the active table.
#[cfg(test)]
fn compact_time(time: &str) -> Option<String> {
    compact_time_in(time, &HANS)
}

fn compact_time_in(time: &str, table: &HashMap<String, String>) -> Option<String> {
    let mut spoken = String::new();
    let mut matched = false;
    for token in time.split(' ') {
        let mut chars = token.chars();
        let unit = chars.next_back()?;
        let n: u32 = chars.as_str().parse().ok()?;
        let word = match unit {
            'd' => translate_with("day unit", table),
            'h' => translate_with("hour unit", table),
            'm' => translate_with("minute unit", table),
            _ => return None,
        };
        matched = true;
        if n == 0 {
            continue;
        }
        spoken.push_str(&n.to_string());
        spoken.push_str(word.as_ref());
    }
    if !matched {
        return None;
    }
    if spoken.is_empty() {
        spoken = format!("0{}", translate_with("minute unit", table));
    }
    Some(spoken)
}

/// Fills a template's `{key}`s in one pass: a device named "{code}" stays as written.
pub(in crate::tui) fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let values = values.iter().map(|(key, value)| (format!("{{{key}}}"), (*value).to_owned())).collect();
    substitute(template, &values)
}

/// An exact chrome phrase. Chat text that is not one of those phrases stays as written.
pub(in crate::tui) fn tr(text: &str) -> Cow<'_, str> {
    match active_phrases() {
        Some(table) => translate_with(text, table),
        None => Cow::Borrowed(text),
    }
}

/// One piece of a phrase key such as "Runs the official installer on {name}."
struct Piece {
    literal: Option<String>,
    token: Option<String>,
}

struct Template {
    key: String,
    pieces: Vec<Piece>,
    literal: usize,
}

/// Phrase keys that already contain `{token}` slots, longest fixed text first.
static TEMPLATES: LazyLock<Vec<Template>> = LazyLock::new(|| {
    let mut out = Vec::new();
    for key in HANS.keys() {
        let Some(pieces) = parse_template(key) else { continue };
        let literal: usize = pieces.iter().map(|piece| piece.literal.as_ref().map_or(0, String::len)).sum();
        if literal < 6 {
            continue;
        }
        out.push(Template { key: key.clone(), pieces, literal });
    }
    out.sort_by(|a, b| b.literal.cmp(&a.literal).then(b.key.len().cmp(&a.key.len())));
    out
});

/// `{name}` and the same shape. A `{` that is not a slot makes the key unusable as a template.
fn parse_template(template: &str) -> Option<Vec<Piece>> {
    let mut pieces = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        if open > 0 {
            pieces.push(Piece { literal: Some(rest[..open].to_owned()), token: None });
        }
        let close = rest[open..].find('}')?;
        let token = &rest[open..=open + close];
        let name = &token[1..token.len() - 1];
        if name.is_empty()
            || !name.chars().all(|c| c.is_ascii_alphanumeric())
            || pieces.last().is_some_and(|piece| piece.token.is_some())
        {
            return None;
        }
        pieces.push(Piece { literal: None, token: Some(token.to_owned()) });
        rest = &rest[open + close + 1..];
    }
    if !rest.is_empty() {
        pieces.push(Piece { literal: Some(rest.to_owned()), token: None });
    }
    pieces.iter().any(|piece| piece.token.is_some()).then_some(pieces)
}

fn phrase_of<'a>(table: &'a HashMap<String, String>, key: &str) -> Option<&'a String> {
    // Only Traditional retains the legacy Simplified fallback; other languages keep English.
    table.get(key).or_else(|| if std::ptr::eq(table, &raw const *HANT) { HANS.get(key) } else { None })
}

/// A sentence the host or a view already filled in, matched back to its `{token}` phrase.
fn templated(text: &str, table: &HashMap<String, String>) -> Option<String> {
    templated_at(text, table, 0)
}

fn templated_at(text: &str, table: &HashMap<String, String>, depth: u8) -> Option<String> {
    for template in TEMPLATES.iter() {
        let Some(caps) = captures(text, &template.pieces) else { continue };
        let Some(phrase) = phrase_of(table, &template.key) else { continue };
        let mut values = HashMap::new();
        for (token, value) in caps {
            // Only app-owned UI slots translate recursively; names and free text stay verbatim.
            let localizable = matches!(
                token.as_str(),
                "{hint}"
                    | "{error}"
                    | "{setup}"
                    | "{kind}"
                    | "{when}"
                    | "{status}"
                    | "{reason}"
                    | "{shortcut}"
                    | "{changes}"
            );
            let shown = if !localizable {
                value
            } else if let Some(hit) = phrase_of(table, &value) {
                hit.clone()
            } else if depth == 0 {
                templated_at(&value, table, 1).unwrap_or(value)
            } else {
                value
            };
            values.insert(token, shown);
        }
        return Some(substitute(phrase, &values));
    }
    None
}

/// Substitute once, so a name containing another slot's spelling stays literal.
fn substitute(phrase: &str, values: &HashMap<String, String>) -> String {
    let mut result = String::new();
    for piece in phrase.split_inclusive('}') {
        let Some(open) = piece.rfind('{') else {
            result.push_str(piece);
            continue;
        };
        result.push_str(&piece[..open]);
        let token = &piece[open..];
        result.push_str(values.get(token).map_or(token, String::as_str));
    }
    result
}

fn captures(text: &str, pieces: &[Piece]) -> Option<Vec<(String, String)>> {
    let mut pos = 0;
    let mut caps = Vec::new();
    for (index, piece) in pieces.iter().enumerate() {
        if let Some(literal) = &piece.literal {
            if !text[pos..].starts_with(literal) {
                return None;
            }
            pos += literal.len();
            continue;
        }
        let token = piece.token.as_deref()?;
        let end = if let Some(next) = pieces.get(index + 1).and_then(|next| next.literal.as_deref()) {
            let rel = text[pos..].find(next)?;
            if rel == 0 {
                return None;
            }
            pos + rel
        } else if pos >= text.len() {
            return None;
        } else {
            text.len()
        };
        caps.push((token.to_owned(), text[pos..end].to_owned()));
        pos = end;
    }
    (pos == text.len()).then_some(caps)
}

#[cfg(test)]
fn translate(text: &str, chinese: bool) -> Cow<'_, str> {
    if chinese { translate_with(text, &HANS) } else { Cow::Borrowed(text) }
}

fn translate_with<'a>(text: &'a str, table: &HashMap<String, String>) -> Cow<'a, str> {
    if let Some(hit) = phrase_of(table, text).cloned().or_else(|| templated(text, table)) {
        return Cow::Owned(hit);
    }
    let trimmed = text.trim();
    if trimmed == text {
        return Cow::Borrowed(text);
    }
    phrase_of(table, trimmed).cloned().or_else(|| templated(trimmed, table)).map_or(Cow::Borrowed(text), Cow::Owned)
}

/// The choice shown beside the palette row.
pub(in crate::tui) fn language_title() -> Cow<'static, str> {
    // Copy the choice out first. A second lock of `PREFERENCE` while this one is held freezes
    // the terminal, and the palette asks for this title on every row.
    let preference = *PREFERENCE.locked();
    match preference {
        AppLanguage::Zh => Cow::Borrowed("简体中文"),
        AppLanguage::ZhHant => Cow::Borrowed("繁體中文"),
        AppLanguage::En => Cow::Borrowed("English"),
        AppLanguage::Ja => Cow::Borrowed("日本語"),
        AppLanguage::Ko => Cow::Borrowed("한국어"),
        AppLanguage::Fr => Cow::Borrowed("Français"),
        AppLanguage::De => Cow::Borrowed("Deutsch"),
        AppLanguage::Es => Cow::Borrowed("Español"),
        AppLanguage::PtBr => Cow::Borrowed("Português (Brasil)"),
        AppLanguage::Ru => Cow::Borrowed("Русский"),
        AppLanguage::It => Cow::Borrowed("Italiano"),
        AppLanguage::System => tr("Follow system"),
    }
}

/// Cycle all supported languages and keep the choice for the next run.
pub(in crate::tui) fn cycle_language() -> Cow<'static, str> {
    let next = {
        let mut preference = PREFERENCE.locked();
        let next = preference.next();
        *preference = next;
        next
    };
    if let Err(error) = super::choice::write(FILE, next.stored()) {
        tracing::warn!(%error, "couldn't save the terminal language");
    }
    language_title()
}

#[cfg(test)]
#[path = "l10n_tests.rs"]
mod registration_tests;
