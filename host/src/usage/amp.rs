//! `amp usage --no-color`: the free allowance as a used share, and any
//! balance that has no ceiling as text.

use std::time::Duration;

use super::Window;

pub(super) async fn collect() -> Option<Vec<Window>> {
    let bin = super::command::program("amp")?;
    let text = super::command::output(&bin, &["usage", "--no-color"], None, Duration::from_secs(8)).await?;
    let windows = parse(&text);
    if windows.is_empty() {
        tracing::debug!("amp usage output had no credit lines");
        None
    } else {
        Some(windows)
    }
}

fn parse(text: &str) -> Vec<Window> {
    let mut windows = Vec::new();
    for line in text.lines() {
        let Some((raw, rest)) = line.split_once(':') else { continue };
        let rest = rest.trim();
        let Some(money) = rest.strip_prefix('$') else { continue };
        let label = amp_label(raw.trim());
        let id = label.to_ascii_lowercase().replace(' ', "_");
        // "$17.59/$20 remaining …" or "$0 remaining …".
        let head = money.split(" remaining").next().unwrap_or(money).trim();
        if let Some((left, total)) = head.split_once('/') {
            let (Some(left), Some(total)) = (dollars(left), dollars(total.trim_start_matches('$'))) else { continue };
            if total <= 0.0 {
                continue;
            }
            let percent = ((total - left) / total * 100.0).clamp(0.0, 100.0);
            windows.push(Window::new(id, label, percent, None));
        } else if let Some(left) = dollars(head) {
            windows.push(Window::note(id, label, format!("${left:.2} left")));
        }
    }
    windows
}

fn amp_label(raw: &str) -> String {
    match raw {
        "Amp Free" => "Free".to_owned(),
        "Individual credits" => "Individual".to_owned(),
        other => other.to_owned(),
    }
}

fn dollars(text: &str) -> Option<f64> {
    let text = text.trim().trim_start_matches('$');
    let n: f64 = text.parse().ok()?;
    n.is_finite().then_some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_allowance_is_the_share_used_and_a_balance_is_text() {
        let text = "\
Signed in as user@example.com (username)\n\
Amp Free: $17.59/$20 remaining (replenishes +$0.83/hour)\n\
Individual credits: $0 remaining - https://ampcode.com\n";
        let w = parse(text);
        assert_eq!(w[0].id, "free");
        assert!((w[0].percent - 12.05).abs() < 0.01, "{}", w[0].percent);
        assert_eq!(w[1].text.as_deref(), Some("$0.00 left"));
    }
}
