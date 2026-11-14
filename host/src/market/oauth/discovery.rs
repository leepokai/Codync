//! Finding out how a remote MCP server signs in: whether it wants sign-in at all, its protected
//! resource metadata (RFC 9728) and its authorization server's metadata (RFC 8414).

use super::TIMEOUT;
use anyhow::{Result, bail};
use serde_json::{Value, json};

async fn get_json(url: &str) -> Result<Value> {
    let res = crate::http().get(url).header("accept", "application/json").timeout(TIMEOUT).send().await?;
    if !res.status().is_success() {
        bail!("{url} answered {}", res.status());
    }
    Ok(res.json().await?)
}

/// `true` when the server wants sign-in: an unauthenticated `initialize` gets 401, or the server
/// lets `initialize` through, refuses only tool calls, and says so in its resource metadata.
pub async fn required(url: &str, headers: &std::collections::BTreeMap<String, String>) -> bool {
    let mut req = crate::http()
        .post(url)
        .header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {
            "protocolVersion": crate::mcp::PROTOCOL_VERSION, "capabilities": {},
            "clientInfo": {"name": "codync", "version": env!("CARGO_PKG_VERSION")},
        }}))
        .timeout(TIMEOUT);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    let Ok(res) = req.send().await else { return false };
    if res.status() == reqwest::StatusCode::UNAUTHORIZED {
        return true;
    }
    if res.status().is_success() {
        // Headers the user gave (an API key) are their way in; don't ask for sign-in on top.
        return headers.is_empty()
            && resource_metadata(url).await["authorization_servers"].as_array().is_some_and(|a| !a.is_empty());
    }
    if !matches!(res.status().as_u16(), 400 | 404 | 405) {
        return false;
    }
    // Not streamable HTTP: ask the older SSE endpoint instead (the stream is dropped unread).
    let mut req = crate::http().get(url).header("accept", "text/event-stream").timeout(TIMEOUT);
    for (k, v) in headers {
        req = req.header(k, v);
    }
    req.send().await.is_ok_and(|r| r.status() == reqwest::StatusCode::UNAUTHORIZED)
}

/// `resource_metadata="…"` from a `WWW-Authenticate: Bearer …` header.
fn resource_metadata_hint(header: &str) -> Option<String> {
    let (_, rest) = header.split_once("resource_metadata=")?;
    let rest = rest.trim_start_matches('"');
    let end = rest.find(['"', ',', ' ']).unwrap_or(rest.len());
    Some(rest[..end].to_owned()).filter(|s| s.starts_with("https://") || s.starts_with("http://"))
}

/// `https://a.b/x/y` → (`https://a.b`, `/x/y`), without a trailing `/`.
pub(super) fn split(url: &str) -> Result<(String, String)> {
    let u = reqwest::Url::parse(url)?;
    let path = u.path().trim_end_matches('/').to_owned();
    Ok((u.origin().ascii_serialization(), path))
}

/// The protected resource metadata (RFC 9728): which authorization server, which scopes.
pub(super) async fn resource_metadata(url: &str) -> Value {
    let hint = crate::http()
        .post(url)
        .header("accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc": "2.0", "id": 0, "method": "ping"}))
        .timeout(TIMEOUT)
        .send()
        .await
        .ok()
        .and_then(|r| r.headers().get("www-authenticate")?.to_str().ok().and_then(resource_metadata_hint));
    let mut tries: Vec<String> = hint.into_iter().collect();
    if let Ok((origin, path)) = split(url) {
        tries.push(format!("{origin}/.well-known/oauth-protected-resource{path}"));
        tries.push(format!("{origin}/.well-known/oauth-protected-resource"));
    }
    for t in tries {
        if let Ok(v) = get_json(&t).await {
            return v;
        }
    }
    Value::Null
}

/// The authorization server metadata (RFC 8414, then `OpenID` discovery).
pub(super) async fn server_metadata(issuer: &str) -> Result<Value> {
    let (origin, path) = split(issuer)?;
    for t in [
        format!("{origin}/.well-known/oauth-authorization-server{path}"),
        format!("{origin}/.well-known/openid-configuration{path}"),
        format!("{issuer}/.well-known/openid-configuration"),
    ] {
        if let Ok(v) = get_json(&t).await
            && v["authorization_endpoint"].is_string()
        {
            return Ok(v);
        }
    }
    bail!("{origin} doesn't describe how to sign in")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_resource_metadata_hint() {
        let h = r#"Bearer error="invalid_token", resource_metadata="https://mcp.example.com/.well-known/oauth-protected-resource""#;
        assert_eq!(
            resource_metadata_hint(h).as_deref(),
            Some("https://mcp.example.com/.well-known/oauth-protected-resource")
        );
        assert_eq!(resource_metadata_hint("Bearer realm=\"x\""), None);
        assert_eq!(resource_metadata_hint("Bearer resource_metadata=\"javascript:x\""), None);
    }

    #[test]
    fn splits_origin_and_path() {
        assert_eq!(split("https://mcp.linear.app/mcp/").unwrap(), ("https://mcp.linear.app".into(), "/mcp".into()));
        assert_eq!(split("https://a.b").unwrap(), ("https://a.b".into(), String::new()));
    }

    /// Serves `initialize` with `status`, and protected resource metadata when `metadata`.
    async fn server(status: u16, metadata: bool) -> String {
        use axum::{Json, Router, http::StatusCode, routing};
        let code = StatusCode::from_u16(status).unwrap();
        let mut router = Router::new().route("/mcp", routing::post(move || async move { (code, Json(json!({}))) }));
        if metadata {
            router = router.route(
                "/.well-known/oauth-protected-resource",
                routing::get(|| async { Json(json!({"authorization_servers": ["https://auth.example"]})) }),
            );
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/mcp", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        url
    }

    #[tokio::test]
    async fn sign_in_is_required_when_the_server_asks_for_it() {
        let none = std::collections::BTreeMap::new();
        assert!(required(&server(401, false).await, &none).await);
        assert!(!required(&server(200, false).await, &none).await);
        // Lets `initialize` through, refuses tool calls, and publishes its metadata.
        let lazy = server(200, true).await;
        assert!(required(&lazy, &none).await);
        let key = std::collections::BTreeMap::from([("x-api-key".to_owned(), "k".to_owned())]);
        assert!(!required(&lazy, &key).await);
    }
}
