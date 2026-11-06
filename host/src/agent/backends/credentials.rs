//! What each provider's CLI keeps about its sign-in: whether it's signed in, and with which
//! account. Read from the files the CLI itself writes — no process started, no browser, no
//! network. A missing file says "signed out" only when the CLI is installed: an installed CLI
//! with no credentials is signed out; without one, nothing is known.

use serde_json::Value;
use std::path::{Path, PathBuf};

use super::{harness, on_path};

/// The sign-in state read from an agent's own files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Credential {
    /// `None`: no evidence either way.
    pub signed_in: Option<bool>,
    /// The account, when the file names it.
    pub email: Option<String>,
}

/// The credentials `id`'s CLI keeps on this machine, for the providers we know by name.
pub fn for_id(id: &str) -> Credential {
    let Some(home) = dirs::home_dir() else { return Credential::default() };
    let installed = harness(id).is_some_and(|h| h.bins.iter().any(|b| on_path(b)));
    match id {
        "claude" => claude(&home, installed),
        "codex" => codex(&home, installed),
        "gemini" => gemini(&home, installed),
        "grok" => grok(&home, installed),
        "opencode" => opencode(&home, &opencode_candidates(&home), installed),
        "kimi" => kimi(&home, installed),
        "antigravity" => antigravity(&home, installed),
        _ => Credential::default(),
    }
}

/// Where OpenCode keeps its files: its XDG data folder, or Windows' `APPDATA`.
fn opencode_data_dirs(home: &Path) -> Vec<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| home.join(".local").join("share"), PathBuf::from);
    let mut dirs = vec![base.join("opencode")];
    if let Some(appdata) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("opencode"));
    }
    dirs
}

fn opencode_candidates(home: &Path) -> Vec<PathBuf> {
    opencode_data_dirs(home).into_iter().map(|dir| dir.join("auth.json")).collect()
}

/// Signed out when the CLI is installed; unknown when it isn't.
fn absent(installed: bool) -> Credential {
    if installed { Credential { signed_in: Some(false), email: None } } else { Credential::default() }
}

fn non_empty(s: &str) -> bool {
    !s.trim().is_empty()
}

fn read(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// The `email` claim of a JWT's payload, its base64url section.
fn jwt_email(token: &str) -> Option<String> {
    use base64::Engine;
    let payload = token.split('.').nth(1)?;
    let b64 = payload.replace('-', "+").replace('_', "/");
    let b64 = match b64.len() % 4 {
        2 => format!("{b64}=="),
        3 => format!("{b64}="),
        _ => b64,
    };
    let bytes = base64::engine::general_purpose::STANDARD.decode(&b64).ok()?;
    let claims: Value = serde_json::from_slice(&bytes).ok()?;
    claims.get("email").and_then(Value::as_str).filter(|s| non_empty(s)).map(str::to_owned)
}

/// Claude Code keeps its sign-in in `~/.claude/.credentials.json`, under one of the `*Oauth`
/// blocks. The access token names the account when it's a JWT; on a subscription login it's an
/// opaque token and the account lives in `~/.claude.json` instead.
fn claude(home: &Path, installed: bool) -> Credential {
    let Some(v) = read(&home.join(".claude").join(".credentials.json")) else {
        return absent(installed);
    };
    for key in ["claudeAiOauth", "claudeProOauth", "claudeMaxOauth"] {
        if let Some(token) = v[key]["accessToken"].as_str().filter(|t| non_empty(t)) {
            let email = jwt_email(token).or_else(|| {
                read(&home.join(".claude.json"))
                    .and_then(|v| v["oauthAccount"]["emailAddress"].as_str().map(str::to_owned))
            });
            return Credential { signed_in: Some(true), email };
        }
    }
    absent(installed)
}

/// Codex keeps its sign-in in `~/.codex/auth.json`: a ChatGPT login (`auth_mode: "chatgpt"`,
/// whose id token is a JWT naming the account) or a bare API key.
fn codex(home: &Path, installed: bool) -> Credential {
    let Some(v) = read(&home.join(".codex").join("auth.json")) else { return absent(installed) };
    match v["auth_mode"].as_str() {
        Some("chatgpt") => {
            let tokens = &v["tokens"];
            let jwt = tokens["id_token"]
                .as_str()
                .filter(|t| non_empty(t))
                .or_else(|| tokens["access_token"].as_str().filter(|t| non_empty(t)));
            let Some(jwt) = jwt else { return absent(installed) };
            Credential { signed_in: Some(true), email: jwt_email(jwt) }
        }
        Some("apikey") if v["OPENAI_API_KEY"].as_str().is_some_and(non_empty) => {
            Credential { signed_in: Some(true), email: None }
        }
        _ => absent(installed),
    }
}

/// Gemini CLI keeps its sign-in in `~/.gemini/oauth_creds.json`; a refresh token means signed
/// in, and the id token is a JWT naming the account.
fn gemini(home: &Path, installed: bool) -> Credential {
    let Some(v) = read(&home.join(".gemini").join("oauth_creds.json")) else {
        return absent(installed);
    };
    if v["refresh_token"].as_str().is_none_or(|t| !non_empty(t)) {
        return absent(installed);
    }
    let email = v["id_token"].as_str().and_then(jwt_email).or_else(|| v["email"].as_str().map(str::to_owned));
    Credential { signed_in: Some(true), email }
}

/// Grok keeps its sign-in in `~/.grok/auth.json`: one entry per `<issuer>::<client>`, the
/// newer builds nesting the fields in an object, the older ones as a flat `<entry>.<field>`
/// map. A refresh or access token means signed in, and the `email` field names the account.
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "field names in Grok's key map, not file extensions"
)]
fn grok(home: &Path, installed: bool) -> Credential {
    let Some(map) = read(&home.join(".grok").join("auth.json")).and_then(|v| v.as_object().cloned()) else {
        return absent(installed);
    };
    let has_token = |o: &serde_json::Map<String, Value>| {
        ["refresh_token", "access_token"].iter().any(|f| o.get(*f).and_then(Value::as_str).is_some_and(non_empty))
    };
    let signed = map.iter().any(|(k, v)| {
        k.ends_with(".refresh_token") || k.ends_with(".access_token") || v.as_object().is_some_and(has_token)
    });
    if !signed {
        return absent(installed);
    }
    let email = map
        .iter()
        .find_map(|(k, v)| {
            if k.ends_with(".email") {
                v.as_str().map(str::to_owned)
            } else {
                v.as_object().and_then(|o| o.get("email")).and_then(Value::as_str).map(str::to_owned)
            }
        })
        .filter(|e| non_empty(e));
    Credential { signed_in: Some(true), email }
}

/// OpenCode 1.x keeps its sign-ins in `auth.json` under its data folder, one
/// entry per provider. OpenCode 2 keeps the active console connection in its
/// database instead.
fn opencode(home: &Path, candidates: &[PathBuf], installed: bool) -> Credential {
    for path in candidates {
        if let Some(v) = read(path)
            && v.as_object().is_some_and(|m| !m.is_empty())
        {
            return Credential { signed_in: Some(true), email: None };
        }
    }
    opencode_database(home).unwrap_or_else(|| absent(installed))
}

/// The active OpenCode console credential, with the account it names.
fn opencode_database(home: &Path) -> Option<Credential> {
    for dir in opencode_data_dirs(home) {
        let path = match std::env::var_os("OPENCODE_DB").filter(|p| !p.as_os_str().is_empty()) {
            Some(raw) => {
                let path = PathBuf::from(raw);
                if path.is_absolute() { path } else { dir.join(path) }
            }
            None => dir.join("opencode.db"),
        };
        if !path.is_file() {
            continue;
        }
        let Ok(conn) = rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else {
            continue;
        };
        let _ = conn.busy_timeout(std::time::Duration::from_millis(200));
        let Ok(value) = conn.query_row(
            "SELECT value FROM credential
             WHERE integration_id = 'opencode' AND active IS NOT 0
             ORDER BY active DESC, time_created DESC, id DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        ) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<Value>(&value) else { continue };
        let server = value["metadata"]["server"].as_str();
        if server.is_some_and(|server| server.trim_end_matches('/') != "https://opencode.ai/console") {
            continue;
        }
        return Some(Credential {
            signed_in: Some(true),
            email: value["metadata"]["email"].as_str().filter(|s| non_empty(s)).map(str::to_owned),
        });
    }
    None
}

/// Kimi Code keeps one file per signed-in account in `~/.kimi/credentials/`.
fn kimi(home: &Path, installed: bool) -> Credential {
    let Ok(entries) = std::fs::read_dir(home.join(".kimi").join("credentials")) else {
        return absent(installed);
    };
    let any = entries.flatten().any(|entry| {
        let path = entry.path();
        if path.is_file() {
            return true;
        }
        path.is_dir() && std::fs::read_dir(&path).is_ok_and(|mut d| d.next().is_some())
    });
    if any { Credential { signed_in: Some(true), email: None } } else { absent(installed) }
}

/// Antigravity's ACP server signs into its default profile, `~/.gemini/antigravity-acp/`,
/// where `acp_token.json` keeps the OAuth refresh token.
fn antigravity(home: &Path, installed: bool) -> Credential {
    let Some(token) = read(&home.join(".gemini").join("antigravity-acp").join("acp_token.json")) else {
        return absent(installed);
    };
    if token["refresh_token"].as_str().is_some_and(non_empty) {
        return Credential { signed_in: Some(true), email: None };
    }
    absent(installed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn jwt(claims: &Value) -> String {
        use base64::Engine;
        let encode = |v: &Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v.to_string());
        format!("{}.{}.{}", encode(&json!({"alg": "none"})), encode(claims), "signature")
    }

    /// A throwaway home directory holding `files` (relative path → body).
    fn home_with(files: &[(&str, &str)]) -> PathBuf {
        let home = std::env::temp_dir().join(format!("codync-credentials-{}", uuid::Uuid::new_v4()));
        for (rel, body) in files {
            let path = home.join(rel);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, body).unwrap();
        }
        home
    }

    fn home_empty() -> PathBuf {
        std::env::temp_dir().join(format!("codync-credentials-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn a_jwt_payload_gives_its_email_claim() {
        let token = jwt(&json!({"sub": "1", "email": "sebasvilledah25@gmail.com"}));
        assert_eq!(jwt_email(&token), Some("sebasvilledah25@gmail.com".to_owned()));
        assert_eq!(jwt_email("not-a-jwt"), None);
        assert_eq!(jwt_email(&jwt(&json!({"sub": "1"}))), None, "no email claim");
    }

    #[test]
    fn claude_reads_its_oauth_block_and_jwt() {
        let home = home_with(&[(
            ".claude/.credentials.json",
            &format!(
                "{{\"claudeAiOauth\": {{\"accessToken\": \"{}\", \"subscriptionType\": \"pro\"}}}}",
                jwt(&json!({"email": "claude@x.co"}))
            ),
        )]);
        assert_eq!(claude(&home, true), Credential { signed_in: Some(true), email: Some("claude@x.co".to_owned()) });
        let empty = home_with(&[(".claude/.credentials.json", "{\"mcpOAuth\": {}}")]);
        assert_eq!(claude(&empty, true), Credential { signed_in: Some(false), email: None });
        assert_eq!(claude(&home_empty(), false), Credential::default(), "no CLI, no file: unknown");
    }

    #[test]
    fn claude_falls_back_to_the_account_in_its_settings() {
        // A subscription login: the access token is opaque and the account lives in `~/.claude.json`.
        let home = home_with(&[
            (
                ".claude/.credentials.json",
                r#"{"claudeAiOauth": {"accessToken": "sk-ant-oat01-opaque", "subscriptionType": "pro"}}"#,
            ),
            (".claude.json", r#"{"oauthAccount": {"emailAddress": "claude@x.co"}}"#),
        ]);
        assert_eq!(claude(&home, true), Credential { signed_in: Some(true), email: Some("claude@x.co".to_owned()) });
        let no_settings =
            home_with(&[(".claude/.credentials.json", r#"{"claudeAiOauth": {"accessToken": "sk-ant-oat01-opaque"}}"#)]);
        assert_eq!(claude(&no_settings, true), Credential { signed_in: Some(true), email: None });
    }

    #[test]
    fn codex_reads_chatgpt_and_api_key_logins() {
        let chatgpt = home_with(&[(
            ".codex/auth.json",
            &format!(
                "{{\"auth_mode\": \"chatgpt\", \"tokens\": {{\"id_token\": \"{}\"}}}}",
                jwt(&json!({"email": "codex@x.co"}))
            ),
        )]);
        assert_eq!(codex(&chatgpt, true), Credential { signed_in: Some(true), email: Some("codex@x.co".to_owned()) });
        let key = home_with(&[(".codex/auth.json", "{\"auth_mode\": \"apikey\", \"OPENAI_API_KEY\": \"sk-abc\"}")]);
        assert_eq!(codex(&key, true), Credential { signed_in: Some(true), email: None });
        let out = home_with(&[(".codex/auth.json", "{\"auth_mode\": \"chatgpt\", \"tokens\": {}}")]);
        assert_eq!(codex(&out, true), Credential { signed_in: Some(false), email: None });
    }

    #[test]
    fn gemini_reads_its_oauth_creds() {
        let home = home_with(&[(
            ".gemini/oauth_creds.json",
            &format!("{{\"refresh_token\": \"r\", \"id_token\": \"{}\"}}", jwt(&json!({"email": "gemini@x.co"}))),
        )]);
        assert_eq!(gemini(&home, true), Credential { signed_in: Some(true), email: Some("gemini@x.co".to_owned()) });
        let out = home_with(&[(".gemini/oauth_creds.json", "{\"refresh_token\": \"\"}")]);
        assert_eq!(gemini(&out, true), Credential { signed_in: Some(false), email: None });
    }

    #[test]
    fn grok_reads_its_nested_and_flat_maps() {
        // Newer builds: the fields nest under the `<issuer>::<client>` entry.
        let nested = home_with(&[(
            ".grok/auth.json",
            r#"{"https://auth.x.ai::c1": {"auth_mode": "oidc", "refresh_token": "r", "email": "grok@x.co"}}"#,
        )]);
        assert_eq!(grok(&nested, true), Credential { signed_in: Some(true), email: Some("grok@x.co".to_owned()) });
        let flat = home_with(&[(
            ".grok/auth.json",
            r#"{"https://auth.x.ai::c1.refresh_token": "r", "https://auth.x.ai::c1.email": "grok@x.co"}"#,
        )]);
        assert_eq!(grok(&flat, true), Credential { signed_in: Some(true), email: Some("grok@x.co".to_owned()) });
        let out = home_with(&[(".grok/auth.json", r#"{"https://auth.x.ai::c1": {"auth_mode": "oidc"}}"#)]);
        assert_eq!(grok(&out, true), Credential { signed_in: Some(false), email: None });
    }

    #[test]
    fn opencode_reads_its_auth_file() {
        let home = home_with(&[(".local/share/opencode/auth.json", r#"{"anthropic": {"type": "oauth"}}"#)]);
        let path = home.join(".local/share/opencode/auth.json");
        assert_eq!(
            opencode(&home, std::slice::from_ref(&path), true),
            Credential { signed_in: Some(true), email: None }
        );
        let empty = home_with(&[(".local/share/opencode/auth.json", "{}")]);
        let empty_path = empty.join(".local/share/opencode/auth.json");
        assert_eq!(opencode(&empty, &[empty_path], true), Credential { signed_in: Some(false), email: None });
        assert_eq!(opencode(&home, &[home.join("nowhere")], false), Credential::default());
    }

    #[test]
    fn opencode_reads_an_active_database_session() {
        let home = home_empty();
        let dir = home.join(".local/share/opencode");
        std::fs::create_dir_all(&dir).unwrap();
        let conn = rusqlite::Connection::open(dir.join("opencode.db")).unwrap();
        conn.execute(
            "CREATE TABLE credential (id TEXT, integration_id TEXT, value TEXT, active INTEGER, time_created INTEGER)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO credential (id, integration_id, value, active, time_created) VALUES ('c', 'opencode', ?1, 1, 1)",
            [r#"{"type":"oauth","access":"tok","metadata":{"server":"https://opencode.ai/console","email":"me@example.com"}}"#],
        )
        .unwrap();
        drop(conn);
        assert_eq!(
            opencode(&home, &[], true),
            Credential { signed_in: Some(true), email: Some("me@example.com".into()) }
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn opencode_candidates_cover_the_usual_folders() {
        let home = home_empty();
        let candidates = opencode_candidates(&home);
        let suffix = Path::new("opencode").join("auth.json");
        assert!(candidates.iter().any(|p| p.ends_with(&suffix)));
    }

    #[test]
    fn kimi_counts_its_credentials() {
        let home = home_with(&[(".kimi/credentials/account-1", "{}")]);
        assert_eq!(kimi(&home, true), Credential { signed_in: Some(true), email: None });
        let empty = home_with(&[]);
        assert_eq!(kimi(&empty, true), Credential { signed_in: Some(false), email: None }, "no credentials file");
    }

    #[test]
    fn antigravity_reads_its_acp_token() {
        let home =
            home_with(&[(".gemini/antigravity-acp/acp_token.json", r#"{"refresh_token": "r", "project_id": "p"}"#)]);
        assert_eq!(antigravity(&home, true), Credential { signed_in: Some(true), email: None });
        let out = home_with(&[(".gemini/antigravity-acp/acp_token.json", r#"{"refresh_token": ""}"#)]);
        assert_eq!(antigravity(&out, true), Credential { signed_in: Some(false), email: None });
    }
}
