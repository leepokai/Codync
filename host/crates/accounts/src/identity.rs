//! Who a slot is signed in as, read from the files its agent wrote there. Never asked of the provider: that
//! would be a call with the account's own credentials.

use serde_json::Value;
use std::path::Path;

pub(crate) fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn email(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// The claims of a JWT, unverified: for reading who a saved token names, never for trusting it.
pub fn jwt_claims(token: &str) -> Option<Value> {
    use base64::Engine as _;
    use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).or_else(|_| URL_SAFE.decode(payload)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

/// The file that says who a login is: where it is in a slot, where the computer's own is under the user's home,
/// and how to read it.
pub(crate) struct Names {
    pub(crate) in_slot: &'static str,
    pub(crate) own: &'static str,
    pub(crate) read: fn(&Value) -> Option<String>,
}

impl Names {
    pub(crate) fn of(&self, file: &Path) -> Option<String> {
        (self.read)(&read_json(file)?)
    }
}

/// Claude Code's `.claude.json` names the account it signed in. The computer's own sits beside `~/.claude`.
pub(crate) const CLAUDE: Names = Names { in_slot: ".claude.json", own: ".claude.json", read: claude };
/// Codex's `auth.json` holds the sign-in's ID token, whose claims name the account.
pub(crate) const CODEX: Names = Names { in_slot: "auth.json", own: ".codex/auth.json", read: codex };
/// OpenCode's `auth.json` keeps a login per provider; a ChatGPT one names its account in its token's claims.
pub(crate) const OPENCODE: Names =
    Names { in_slot: "data/opencode/auth.json", own: ".local/share/opencode/auth.json", read: opencode };

/// Pi's `auth.json` is laid out like `OpenCode`'s.
pub(crate) const PI: Names = Names { in_slot: "auth.json", own: ".pi/agent/auth.json", read: opencode };
/// Gemini CLI's `google_accounts.json` names the account in use.
pub(crate) const GEMINI: Names =
    Names { in_slot: ".gemini/google_accounts.json", own: ".gemini/google_accounts.json", read: gemini };

fn gemini(accounts: &Value) -> Option<String> {
    email(&accounts["active"])
}

fn claude(state: &Value) -> Option<String> {
    email(&state["oauthAccount"]["emailAddress"])
}

fn codex(auth: &Value) -> Option<String> {
    email(&jwt_claims(auth["tokens"]["id_token"].as_str()?)?["email"])
}

fn opencode(auth: &Value) -> Option<String> {
    auth.as_object()?.values().find_map(|login| {
        let claims = jwt_claims(login["access"].as_str()?)?;
        email(&claims["https://api.openai.com/profile"]["email"]).or_else(|| email(&claims["email"]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::json;

    fn token(claims: &Value) -> String {
        format!("h.{}.s", URL_SAFE_NO_PAD.encode(claims.to_string()))
    }

    #[test]
    fn reads_who_signed_in_from_the_slots_own_files() {
        let slot = crate::slot::tests::scratch();
        let named = |names: &Names| names.of(&slot.join(names.in_slot));
        assert_eq!((named(&CLAUDE), named(&CODEX), named(&OPENCODE)), (None, None, None), "a signed-out slot");
        let write = |path: &str, v: Value| {
            let path = slot.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, v.to_string()).unwrap();
        };
        write(".claude.json", json!({"oauthAccount": {"emailAddress": "a@example.com"}}));
        write("auth.json", json!({"tokens": {"id_token": token(&json!({"email": "b@example.com"}))}}));
        let chatgpt = token(&json!({"https://api.openai.com/profile": {"email": "c@example.com"}}));
        write(
            "data/opencode/auth.json",
            json!({"deepseek": {"type": "api", "key": "k"}, "openai": {"access": chatgpt}}),
        );
        assert_eq!(named(&CLAUDE).as_deref(), Some("a@example.com"));
        assert_eq!(named(&CODEX).as_deref(), Some("b@example.com"));
        assert_eq!(named(&OPENCODE).as_deref(), Some("c@example.com"));
    }
}
