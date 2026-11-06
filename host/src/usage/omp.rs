//! `omp usage --json`: one window per upstream limit oh-my-pi reports.
//! Claude and Codex stay on their own cards; these windows belong to oh-my-pi,
//! labeled with the upstream, so a second login isn't written over the CLI's.

use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

use super::Window;

pub(super) async fn collect() -> Option<Vec<Window>> {
    let bin = super::command::program("omp")?;
    let text = super::command::output(&bin, &["usage", "--json"], None, Duration::from_secs(30)).await?;
    let windows = parse(&text);
    if windows.is_empty() {
        tracing::debug!("omp usage output had no limits");
        None
    } else {
        Some(windows)
    }
}

fn parse(text: &str) -> Vec<Window> {
    let Some(start) = text.find('{') else { return Vec::new() };
    let Some(end) = text.rfind('}') else { return Vec::new() };
    if end <= start {
        return Vec::new();
    }
    let Ok(payload) = serde_json::from_str::<Value>(&text[start..=end]) else { return Vec::new() };
    let Some(reports) = payload["reports"].as_array() else { return Vec::new() };
    let mut per_provider: HashMap<&str, usize> = HashMap::new();
    for report in reports {
        if let Some(id) = report["provider"].as_str() {
            *per_provider.entry(id).or_insert(0) += 1;
        }
    }
    let mut windows = Vec::new();
    for (index, report) in reports.iter().enumerate() {
        let Some(provider) = report["provider"].as_str() else { continue };
        let name = provider_name(provider);
        let suffix = (per_provider.get(provider).copied().unwrap_or(0) > 1).then(|| discriminator(report, index));
        let account = account_key(report, index);
        let mut pools = HashSet::new();
        let Some(limits) = report["limits"].as_array() else { continue };
        for (n, limit) in limits.iter().enumerate() {
            if let Some(pool) = limit["scope"]["sharedGroup"].as_str().filter(|s| !s.is_empty())
                && !pools.insert(pool.to_owned())
            {
                continue;
            }
            let Some(window) = one_limit(&name, suffix.as_deref(), provider, &account, n, limit) else { continue };
            windows.push(window);
        }
    }
    windows
}

fn one_limit(
    name: &str,
    suffix: Option<&str>,
    provider: &str,
    account: &str,
    n: usize,
    limit: &Value,
) -> Option<Window> {
    let amount = &limit["amount"];
    let window_id = limit["scope"]["windowId"].as_str().or_else(|| limit["window"]["id"].as_str()).unwrap_or("limit");
    let raw_label = limit["label"].as_str().map(str::trim).filter(|s| !s.is_empty());
    // The account goes in parentheses: the apps read the part after " · " as Claude's model name
    // and would show "5h · ada" and "Weekly · ada" both as "ada".
    let label = match (raw_label, suffix) {
        (Some(label), Some(suffix)) => format!("{label} ({suffix})"),
        (Some(label), None) => label.to_owned(),
        (None, Some(suffix)) => format!("{name} ({suffix})"),
        (None, None) => name.to_owned(),
    };
    let id = format!("{provider}:{account}:{window_id}:{n}");
    let resets = epoch_ms(&limit["window"]["resetsAt"]);
    if amount["unit"].as_str().is_some_and(|u| u.eq_ignore_ascii_case("usd")) && amount["limit"].is_null() {
        let spent = amount["used"].as_f64()?;
        return Some(Window::note(id, label, format!("${spent:.2} spent")));
    }
    let percent = used_percent(amount)?;
    Some(Window::new(id, label, percent, resets))
}

/// Used share, 0–100. Fractions are 0–1; `used`/`limit` is a count or dollars.
fn used_percent(amount: &Value) -> Option<f64> {
    if let Some(used) = amount["usedFraction"].as_f64() {
        return Some((used * 100.0).clamp(0.0, 100.0));
    }
    if let Some(left) = amount["remainingFraction"].as_f64() {
        return Some(((1.0 - left) * 100.0).clamp(0.0, 100.0));
    }
    let used = amount["used"].as_f64()?;
    let limit = amount["limit"].as_f64().filter(|n| *n > 0.0)?;
    Some((used / limit * 100.0).clamp(0.0, 100.0))
}

fn epoch_ms(v: &Value) -> Option<i64> {
    let n = v.as_i64().or_else(|| v.as_f64().and_then(super::finite_i64))?;
    Some(if n > 100_000_000_000 { n } else { n.saturating_mul(1000) })
}

fn account_key(report: &Value, index: usize) -> String {
    let raw = report["metadata"]["accountId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| report["metadata"]["email"].as_str().filter(|s| !s.is_empty()));
    let Some(raw) = raw else { return format!("account-{}", index + 1) };
    let key: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '@') { c } else { '_' })
        .take(80)
        .collect();
    if key.is_empty() { format!("account-{}", index + 1) } else { key }
}

fn discriminator(report: &Value, index: usize) -> String {
    let email = report["metadata"]["email"].as_str().filter(|s| s.contains('@'));
    if let Some(email) = email {
        return email.split('@').next().unwrap_or(email).chars().take(16).collect();
    }
    report["metadata"]["accountId"].as_str().map_or_else(|| format!("#{}", index + 1), |s| s.chars().take(8).collect())
}

fn provider_name(id: &str) -> String {
    match id {
        "anthropic" => "Claude",
        "openai-codex" => "Codex",
        "zai" => "Z.ai",
        "google-gemini-cli" => "Gemini",
        "google-antigravity" => "Antigravity",
        "github-copilot" => "Copilot",
        "kimi-code" => "Kimi",
        "minimax-code" => "MiniMax",
        "minimax-code-cn" => "MiniMax CN",
        "opencode-go" => "OpenCode Go",
        "alibaba-token-plan" => "Qwen Cloud",
        "xai-oauth" => "Grok",
        other => return title_case(other),
    }
    .to_owned()
}

fn title_case(id: &str) -> String {
    id.split('-')
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_used_fraction_and_a_dollar_cap() {
        let text = r#"status
{"reports":[
  {"provider":"anthropic","limits":[{"label":"Claude 5 Hour","scope":{"windowId":"5h"},
     "window":{"id":"5h","resetsAt":1783885200000},
     "amount":{"usedFraction":0.08,"unit":"percent"}}]},
  {"provider":"xai-oauth","limits":[{"label":"Spend","scope":{"windowId":"monthly"},
     "amount":{"used":4.5,"limit":20,"unit":"usd"}}]}
]}"#;
        let w = parse(text);
        assert_eq!(w[0].label, "Claude 5 Hour");
        assert!((w[0].percent - 8.0).abs() < 1e-9);
        assert_eq!(w[0].resets_at, Some(1_783_885_200_000));
        assert!((w[1].percent - 22.5).abs() < 1e-9);
        assert_eq!(w[1].label, "Spend");
    }

    #[test]
    fn two_accounts_keep_separate_window_ids() {
        let text = r#"{"reports":[
          {"provider":"anthropic","metadata":{"email":"ada@x.com","accountId":"acct_ada"},
           "limits":[{"label":"5h","scope":{"windowId":"5h"},"amount":{"usedFraction":0.1}}]},
          {"provider":"anthropic","metadata":{"email":"grace@x.com","accountId":"acct_grace"},
           "limits":[{"label":"5h","scope":{"windowId":"5h"},"amount":{"usedFraction":0.2}}]}
        ]}"#;
        let w = parse(text);
        assert_eq!(w[0].id, "anthropic:acct_ada:5h:0");
        assert_eq!(w[1].id, "anthropic:acct_grace:5h:0");
        assert_eq!(w[0].label, "5h (ada)");
        assert_eq!(w[1].label, "5h (grace)");
    }

    #[test]
    fn an_uncapped_dollar_spend_is_a_note() {
        let text = r#"{"reports":[{"provider":"openai","limits":[
            {"label":"API","amount":{"used":3.2,"unit":"usd"}}]}]}"#;
        let w = parse(text);
        assert_eq!(w[0].text.as_deref(), Some("$3.20 spent"));
        assert!(w[0].percent.abs() < f64::EPSILON);
    }
}
