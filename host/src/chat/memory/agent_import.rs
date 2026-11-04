//! Importing another agent's memory: facts for a bot's `MEMORY.md`, with the rule that **a credential never lands in
//! memory**. The desktop importer already leaves key-shaped text out; this is the second lock, because the host cannot
//! know that its caller did.
//!
//! The shapes follow Rakazo's `agent-import/credentials.ts` (Apache-2.0; see `THIRD-PARTY-NOTICES.md`), written again here
//! without regular expressions.

use crate::hub::Hub;
use anyhow::{Result, anyhow, bail};
use codync_mem::files::MAIN;
use serde_json::{Value, json};
use std::sync::Arc;

/// The most facts one import adds, and the longest one.
const MAX_FACTS: usize = 400;
const MAX_FACT_CHARS: usize = 2_000;

fn random(value: &str) -> bool {
    let distinct: std::collections::HashSet<char> = value.chars().collect();
    value.chars().any(|c| c.is_ascii_alphabetic()) && value.chars().any(|c| c.is_ascii_digit()) && distinct.len() > 4
}

fn alnum(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn word(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// One whitespace- or punctuation-delimited token that has the shape of a well-known key.
fn token_is_key(token: &str) -> bool {
    let after = |prefix: &str| token.strip_prefix(prefix);
    if let Some(rest) = after("sk-") {
        // `sk-`, `sk-ant-…`, `sk-or-v1-…`, `sk-proj-…`: 20 or more key characters that look random.
        return rest.len() >= 20 && word(rest) && random(rest);
    }
    if let Some(rest) = after("github_pat_") {
        return rest.len() >= 22 && word(rest);
    }
    for prefix in ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"] {
        if let Some(rest) = after(prefix) {
            return rest.len() >= 36 && alnum(rest);
        }
    }
    if let Some(rest) = after("xox") {
        let mut chars = rest.chars();
        return chars.next().is_some_and(|c| "abposr".contains(c))
            && chars.as_str().strip_prefix('-').is_some_and(|r| r.len() >= 10);
    }
    for prefix in ["AKIA", "ASIA"] {
        if let Some(rest) = after(prefix) {
            return rest.len() == 16 && rest.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
        }
    }
    if let Some(rest) = after("AIza") {
        return rest.len() >= 35 && word(rest);
    }
    if let Some(rest) = after("glpat-") {
        return rest.len() >= 20 && word(rest);
    }
    if let Some(rest) = after("hf_") {
        return rest.len() >= 30 && alnum(rest);
    }
    for prefix in ["sk_live_", "rk_live_"] {
        if let Some(rest) = after(prefix) {
            return rest.len() >= 20 && alnum(rest);
        }
    }
    // A Telegram bot token: digits, a colon and 30 or more key characters.
    if let Some((id, secret)) = token.split_once(':') {
        return id.len() >= 6 && id.bytes().all(|b| b.is_ascii_digit()) && secret.len() >= 30 && word(secret);
    }
    false
}

/// Whether text has the shape of a key, token or private key. Never returns or logs the value.
pub fn looks_like_credential(text: &str) -> bool {
    if text.contains("-----BEGIN ") && text.contains("PRIVATE KEY-----") {
        return true;
    }
    let tokens: Vec<&str> = text
        .split(|c: char| !(c.is_ascii_alphanumeric() || "_-:.~+/=".contains(c)))
        .filter(|t| !t.is_empty())
        .collect();
    tokens.iter().enumerate().any(|(i, token)| {
        token_is_key(token.trim_matches(['.', ':', '=']))
            // `Bearer <token>`: 24 or more token characters that look random.
            || (token.eq_ignore_ascii_case("bearer")
                && tokens.get(i + 1).is_some_and(|next| next.trim_end_matches('=').len() >= 24 && random(next)))
    })
}

/// Adds `facts` to the bot's own `MEMORY.md`, one line each, leaving out empty ones, long ones and any that look like a
/// credential. Answers how many were added and how many left out; a full `MEMORY.md` stops the import and says so.
pub async fn import(hub: &Arc<Hub>, bot: &str, facts: &Value) -> Result<Value> {
    let cfg = hub
        .store
        .bot(bot)?
        .filter(|b| !b.deleted && !b.config.is_group())
        .ok_or_else(|| anyhow!("unknown bot"))?
        .config;
    let facts: Vec<&str> =
        facts.as_array().ok_or_else(|| anyhow!("facts must be a list"))?.iter().filter_map(Value::as_str).collect();
    if facts.len() > MAX_FACTS {
        bail!("That is more than {MAX_FACTS} memories at once; import fewer.");
    }
    let (mut added, mut omitted, mut stopped) = (0_u32, 0_u32, None);
    for fact in facts {
        let line = fact.trim().replace('\n', " ");
        if line.is_empty() || line.chars().count() > MAX_FACT_CHARS || looks_like_credential(&line) {
            omitted += 1;
            continue;
        }
        let args = json!({"file": MAIN, "append": format!("- {}", line.trim_start_matches("- "))});
        match super::file_tools::call(hub, &cfg, "edit_memory", &args).await {
            Ok(_) => added += 1,
            Err(error) => {
                // Usually the file is full: what was added stays, and the rest is told to the person.
                stopped = Some(format!("{error:#}"));
                break;
            }
        }
    }
    Ok(json!({"added": added, "omitted": omitted, "stopped": stopped}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_of_the_usual_providers_are_recognised() {
        for secret in [
            "sk-proj-a1B2c3D4e5F6g7H8i9J0k1L2m3N4",
            "sk-ant-api03-a1B2c3D4e5F6g7H8i9J0k1",
            &format!("ghp_{}", "a".repeat(36)),
            "github_pat_11ABCDEFG0abcdefghijkl_mnop",
            "xoxb-1234567890-abcdefghij",
            "AKIAABCDEFGHIJKLMNOP",
            &format!("AIza{}", "a1B2".repeat(9).chars().take(35).collect::<String>()),
            "glpat-abcdefghijklmnopqrstuvwxyz",
            &format!("hf_{}", "aB3".repeat(11)),
            &format!("sk_live_{}", "a1".repeat(12)),
            "123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw_",
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "Authorization: Bearer abcdEFGH1234abcdEFGH1234abcd",
        ] {
            assert!(looks_like_credential(&format!("my key is {secret} ok")), "{secret}");
        }
    }

    #[test]
    fn ordinary_text_and_placeholders_are_not_credentials() {
        for fine in [
            "Prefers short answers",
            "sk-learn is a library",
            "sk-your-key-here",
            "Bearer <token>",
            "The AKIA prefix marks AWS keys",
            "Works 9:30 to 17:30",
            "ghp_ is the prefix of a GitHub token",
            "Likes the sk-8 connector",
        ] {
            assert!(!looks_like_credential(fine), "{fine}");
        }
    }

    #[test]
    fn a_key_at_the_end_of_a_sentence_still_counts() {
        assert!(looks_like_credential("The token is xoxb-1234567890-abcdefghij."));
    }
}
