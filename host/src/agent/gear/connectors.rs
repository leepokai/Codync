//! MCP servers an agent configures in the user's home. A detail is the command, or
//! only a URL's origin: a path can carry a key (`/api/mcp/s/<key>/mcp`).

use serde_json::{Map, Value};
use std::path::Path;
use toml_edit::{DocumentMut, Item};

use super::{MAX_ITEMS, MCP_BYTES, NAME_CHARS, TEXT_CHARS, clip, homes, read_limited};

use super::Named;

pub(super) fn list(backend: &str, home: &Path) -> Vec<Named> {
    let mut out = Vec::new();
    for file in homes(home, files(backend)) {
        read_file(&file, &mut out);
    }
    super::dedupe(&mut out);
    out.truncate(MAX_ITEMS);
    out
}

fn files(backend: &str) -> &'static [&'static str] {
    match backend {
        "cursor" => &[".cursor/mcp.json"],
        "claude" => &[".claude.json", ".claude/settings.json"],
        "codex" => &[".codex/config.toml"],
        "copilot" => &[".copilot/mcp-config.json"],
        "codebuddy" | "workbuddy" => &[".codebuddy/mcp.json", ".codebuddy/settings.json"],
        "gemini" | "antigravity" => &[".gemini/settings.json"],
        "grok" => &[".grok/mcp.json"],
        _ => &[],
    }
}

fn read_file(path: &Path, out: &mut Vec<Named>) {
    if std::fs::metadata(path).is_ok_and(|meta| meta.is_dir()) {
        return;
    }
    let Some(text) = read_limited(path, MCP_BYTES) else { return };
    if path.extension().is_some_and(|ext| ext == "toml") {
        out.extend(mcp_toml(&text));
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(&text) else { return };
    let Some(servers) = servers_object(path, &value) else { return };
    for (name, server) in servers {
        let Some(server) = server.as_object() else { continue };
        if server.get("enabled").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let name = clip(name, NAME_CHARS);
        if name.is_empty() {
            continue;
        }
        out.push(Named { name, description: String::new(), detail: server_detail(server) });
    }
}

fn servers_object<'a>(path: &Path, value: &'a Value) -> Option<&'a Map<String, Value>> {
    if let Some(servers) = value.get("mcpServers").and_then(Value::as_object) {
        return Some(servers);
    }
    let name = path.file_name()?.to_str()?;
    if !matches!(name, "mcp.json" | "mcp-config.json") {
        return None;
    }
    value.get("servers").and_then(Value::as_object).or_else(|| value.as_object())
}

fn server_detail(server: &Map<String, Value>) -> String {
    if let Some(url) = server.get("url").and_then(Value::as_str).filter(|url| !url.is_empty()) {
        return clip(&visible_url(url), TEXT_CHARS);
    }
    clip(&command_text(server), TEXT_CHARS)
}

fn command_text(server: &Map<String, Value>) -> String {
    let mut parts = Vec::new();
    match server.get("command") {
        Some(Value::String(command)) if !command.is_empty() => parts.push(command.clone()),
        Some(Value::Array(command)) => parts.extend(command.iter().filter_map(|v| v.as_str().map(str::to_owned))),
        _ => {}
    }
    if let Some(args) = server.get("args").and_then(Value::as_array) {
        parts.extend(args.iter().filter_map(|v| v.as_str().map(str::to_owned)));
    }
    keep_public(&parts)
}

/// A command without its credentials. Each argument is split into words, so a `sh -c "…"` script
/// is filtered too. Dropped: a header (`Authorization: Bearer …`), a flag or `NAME=value` named like
/// a credential and the value after such a flag (`-H`, `-u` too), and a long letter-and-digit run.
/// A URL keeps only its origin, since a path can carry a key.
fn keep_public(parts: &[String]) -> String {
    let mut kept = Vec::new();
    let mut skip_value = false;
    for part in parts {
        if header(part) {
            skip_value = false;
            continue;
        }
        for word in part.split_whitespace() {
            if std::mem::take(&mut skip_value) {
                continue;
            }
            let raw = word.trim_matches(['"', '\'']);
            let (name, value) = raw.split_once('=').map_or((raw, None), |(name, value)| (name, Some(value)));
            if credential_name(name) || matches!(raw, "-H" | "-u") {
                skip_value = raw.starts_with('-') && value.is_none();
            } else if let Some(url) = value.filter(|v| v.contains("://")) {
                kept.push(format!("{name}={}", visible_url(url)));
            } else if raw.contains("://") {
                kept.push(visible_url(raw));
            } else if !token_run(raw) {
                kept.push(raw.to_owned());
            }
        }
    }
    kept.retain(|word| !word.is_empty());
    kept.join(" ")
}

/// `Authorization: Bearer …`, `X-Api-Key: …`: a whole header, dropped before it splits into words.
fn header(part: &str) -> bool {
    let lower = part.to_ascii_lowercase();
    lower.contains("bearer ")
        || lower.contains("basic ")
        || part.split_once(':').is_some_and(|(name, _)| !name.contains(char::is_whitespace) && credential_name(name))
}

fn credential_name(name: &str) -> bool {
    let name = name.trim_start_matches('-').to_ascii_lowercase().replace('-', "_");
    name.contains("api_key")
        || name.split('_').any(|seg| {
            matches!(
                seg,
                "token"
                    | "tokens"
                    | "secret"
                    | "secrets"
                    | "password"
                    | "passwd"
                    | "pass"
                    | "pwd"
                    | "authorization"
                    | "apikey"
                    | "auth"
                    | "key"
                    | "keys"
                    | "header"
                    | "headers"
                    | "cookie"
                    | "cookies"
                    | "credential"
                    | "credentials"
                    | "user"
                    | "bearer"
            )
        })
}

/// A bare argument that is a long mix of letters and digits, such as an API key.
fn token_run(part: &str) -> bool {
    let s = part.trim().trim_matches(['"', '\'']);
    if s.starts_with('-') || s.chars().count() < 20 {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
        && s.chars().any(|c| c.is_ascii_alphabetic())
        && s.chars().any(|c| c.is_ascii_digit())
}

/// Scheme and host only. The path, user and query stay out.
fn visible_url(raw: &str) -> String {
    let Ok(url) = url::Url::parse(raw) else { return String::new() };
    match url.origin() {
        url::Origin::Tuple(..) => url.origin().ascii_serialization(),
        url::Origin::Opaque(_) => String::new(),
    }
}

/// `[mcp_servers.<name>]` tables in Codex's `config.toml`. `enabled = false` leaves the server out.
fn mcp_toml(text: &str) -> Vec<Named> {
    let Ok(doc) = text.parse::<DocumentMut>() else { return Vec::new() };
    let Some(servers) = doc.get("mcp_servers").and_then(Item::as_table_like) else { return Vec::new() };
    servers
        .iter()
        .filter(|(_, server)| server.get("enabled").and_then(Item::as_bool) != Some(false))
        .filter_map(|(name, server)| {
            let name = clip(name, NAME_CHARS);
            (!name.is_empty()).then(|| Named {
                name,
                description: String::new(),
                detail: clip(&toml_detail(server), TEXT_CHARS),
            })
        })
        .collect()
}

fn toml_detail(server: &Item) -> String {
    if let Some(url) = server.get("url").and_then(Item::as_str) {
        return visible_url(url);
    }
    let mut parts: Vec<String> = server.get("command").and_then(Item::as_str).map(str::to_owned).into_iter().collect();
    if let Some(args) = server.get("args").and_then(Item::as_array) {
        parts.extend(args.iter().filter_map(|arg| arg.as_str().map(str::to_owned)));
    }
    keep_public(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-gear-mcp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn a_large_claude_file_and_a_secret_argument_stay_readable() {
        let root = scratch();
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".claude")).expect("claude");
        let pad = "x".repeat(300 * 1024);
        let body = format!(
            r#"{{"history":"{pad}","mcpServers":{{"github":{{"command":"npx","args":["--api-key","abc123","-y","server","sk-abc123def456ghij78","--token=abc123","API_KEY=abc123","--header","Authorization: Bearer abc123","https://hooks.zapier.com/api/mcp/s/abc123/mcp"]}},"off":{{"command":"nope","enabled":false}},"shell":{{"command":"sh","args":["-c","mcp-server --token abc123 --port 3000"]}},"basic":{{"command":"curl","args":["-u","alice:abc123","https://example.test/mcp","--user=alice:abc123"]}},"zap":{{"url":"https://user:secret@hooks.zapier.com/api/mcp/s/secret-token/mcp?token=1"}}}}}}"#
        );
        std::fs::write(home.join(".claude.json"), body).expect("claude.json");
        let connectors = list("claude", &home);
        let mut names = connectors.iter().map(|c| c.name.as_str()).collect::<Vec<_>>();
        names.sort_unstable();
        assert_eq!(names, ["basic", "github", "shell", "zap"]);
        assert_eq!(detail(&connectors, "github"), "npx -y server https://hooks.zapier.com");
        assert_eq!(detail(&connectors, "shell"), "sh -c mcp-server --port 3000");
        assert_eq!(detail(&connectors, "basic"), "curl https://example.test");
        assert_eq!(detail(&connectors, "zap"), "https://hooks.zapier.com");
        let dumped = connectors.iter().map(|c| c.detail.as_str()).collect::<Vec<_>>().join(" ");
        assert!(!dumped.contains("abc123") && !dumped.contains("secret") && !dumped.contains("alice"), "{dumped}");
        let _ = std::fs::remove_dir_all(root);
    }

    fn detail<'a>(connectors: &'a [Named], name: &str) -> &'a str {
        connectors.iter().find(|c| c.name == name).map_or("<missing>", |c| c.detail.as_str())
    }

    #[test]
    fn codex_servers_read_as_toml() {
        let root = scratch();
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".codex")).expect("codex");
        std::fs::write(
            home.join(".codex/config.toml"),
            r#"[mcp_servers.github]
command = "github-mcp"
enabled = false # temporarily off

[mcp_servers.play] # personal
command = "npx"
args = [
  "-y", '@playwright/mcp',
  "--api-key", "abc123",
  "-H", "X-Api-Key: abc123",
]

[mcp_servers."docs.v2"]
url = "https://alice:abc123@example.test/mcp?key=xyz" # kept

[mcp_servers."docs.v2".env]
TOKEN = "secret-token"
"#,
        )
        .expect("toml");
        let connectors = list("codex", &home);
        assert_eq!(connectors.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["play", "docs.v2"]);
        assert_eq!(connectors[0].detail, "npx -y @playwright/mcp");
        assert_eq!(connectors[1].detail, "https://example.test");
        let dumped = format!("{} {}", connectors[0].detail, connectors[1].detail);
        assert!(!dumped.contains("abc123") && !dumped.contains("alice"), "{dumped}");
        let _ = std::fs::remove_dir_all(root);
    }
}
