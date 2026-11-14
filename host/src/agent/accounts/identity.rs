//! Who a slot is signed in as, read from the files its agent wrote there. Never asked of the provider: that
//! would be a call with the account's own credentials.

use super::read_json;
use serde_json::Value;
use std::path::Path;

fn email(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned)
}

/// Claude Code's `.claude.json` names the account it signed in.
pub(super) fn claude(slot: &Path) -> Option<String> {
    email(&read_json(&slot.join(".claude.json"))?["oauthAccount"]["emailAddress"])
}

/// Codex's `auth.json` holds the sign-in's ID token, whose claims name the account.
pub(super) fn codex(slot: &Path) -> Option<String> {
    let auth = read_json(&slot.join("auth.json"))?;
    email(&crate::usage::jwt_claims(auth["tokens"]["id_token"].as_str()?)?["email"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use serde_json::json;

    #[test]
    fn reads_who_signed_in_from_the_slots_own_files() {
        let slot = std::env::temp_dir().join(format!("codync-account-identity-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&slot).expect("temp dir");
        assert_eq!((claude(&slot), codex(&slot)), (None, None), "a signed-out slot");
        std::fs::write(
            slot.join(".claude.json"),
            json!({"oauthAccount": {"emailAddress": "a@example.com"}}).to_string(),
        )
        .expect("claude");
        let claims = URL_SAFE_NO_PAD.encode(json!({"email": "b@example.com"}).to_string());
        std::fs::write(slot.join("auth.json"), json!({"tokens": {"id_token": format!("h.{claims}.s")}}).to_string())
            .expect("codex");
        assert_eq!(claude(&slot).as_deref(), Some("a@example.com"));
        assert_eq!(codex(&slot).as_deref(), Some("b@example.com"));
    }
}
