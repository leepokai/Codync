//! `JetBrains` AI credits, the monthly quota the IDE saves after AI Assistant
//! (and Junie) has been used. The newest IDE file wins. Nothing is requested.

use std::path::{Path, PathBuf};

use super::Window;

const FILE: &str = "options/AIAssistantQuotaManager2.xml";

pub(super) fn collect() -> Option<Vec<Window>> {
    let mut files = Vec::new();
    for root in roots() {
        files.extend(quota_files(&root));
    }
    let newest = files.into_iter().max_by_key(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok())?;
    let text = std::fs::read_to_string(&newest).ok()?;
    let windows = parse(&text);
    (!windows.is_empty()).then_some(windows)
}

fn roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for base in [dirs::config_dir(), dirs::data_local_dir()].into_iter().flatten() {
        roots.push(base.join("JetBrains"));
        roots.push(base.join("Google"));
    }
    roots
}

fn quota_files(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else { return Vec::new() };
    entries.filter_map(Result::ok).map(|entry| entry.path().join(FILE)).filter(|path| path.is_file()).collect()
}

fn parse(xml: &str) -> Vec<Window> {
    let Some(info) = option(xml, "quotaInfo") else { return Vec::new() };
    if let Some("Unknown" | "Error") = info["type"].as_str() {
        return Vec::new();
    }
    let Some(used) = number(&info["current"]) else { return Vec::new() };
    let Some(maximum) = number(&info["maximum"]) else { return Vec::new() };
    if maximum <= 0.0 {
        return Vec::new();
    }
    let percent = (used / maximum * 100.0).clamp(0.0, 100.0);
    let resets = option(xml, "nextRefill").and_then(|refill| {
        refill["next"]
            .as_str()
            .and_then(|text| chrono::DateTime::parse_from_rfc3339(text).ok())
            .map(|t| t.timestamp_millis())
    });
    vec![Window::new("monthly", "AI credits", percent, resets)]
}

fn option(xml: &str, name: &str) -> Option<serde_json::Value> {
    let marker = format!("name=\"{name}\"");
    let at = xml.find(&marker)?;
    let value_at = xml[at..].find("value=\"")? + at + "value=\"".len();
    let end = xml[value_at..].find('"')? + value_at;
    let text = decode(&xml[value_at..end]);
    serde_json::from_str(&text).ok()
}

fn decode(text: &str) -> String {
    let mut out = text.replace("&#10;", "\n");
    // Numeric entities, then the named ones. `&amp;` is last so `&quot;` survives.
    let mut decoded = String::new();
    let mut rest = out.as_str();
    while let Some(at) = rest.find("&#") {
        decoded.push_str(&rest[..at]);
        let after = &rest[at + 2..];
        let Some(end) = after.find(';') else {
            decoded.push_str(&rest[at..]);
            rest = "";
            break;
        };
        if let Ok(code) = after[..end].parse::<u32>()
            && let Some(c) = char::from_u32(code)
        {
            decoded.push(c);
        }
        rest = &after[end + 1..];
    }
    decoded.push_str(rest);
    out = decoded;
    out = out.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">");
    out.replace("&amp;", "&")
}

fn number(value: &serde_json::Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str().and_then(|text| text.trim().parse().ok())).filter(|n| n.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ide_file_is_a_used_share_with_its_refill() {
        let xml = r#"<application><component name="AIAssistantQuotaManager2">
            <option name="quotaInfo" value="{&quot;type&quot;:&quot;Known&quot;,&quot;current&quot;:&quot;10&quot;,&quot;maximum&quot;:&quot;100&quot;}" />
            <option name="nextRefill" value="{&quot;next&quot;:&quot;2026-11-01T00:00:00Z&quot;}" />
            </component></application>"#;
        let w = parse(xml);
        assert_eq!(w[0].label, "AI credits");
        assert!((w[0].percent - 10.0).abs() < 1e-9);
        assert!(w[0].resets_at.is_some());
    }

    #[test]
    fn an_ide_that_failed_to_fetch_is_not_a_zero() {
        let xml = r#"<option name="quotaInfo" value="{&quot;type&quot;:&quot;Error&quot;}" />"#;
        assert_eq!(parse(xml), Vec::new());
    }
}
