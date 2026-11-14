//! A Codync Cloud computer's remote connectors: their secrets (literal header values, sign-in tokens) are its
//! keeper's (`crate::keeper`, proxied slot `connector:<id>`, sent only to the connector's origin), and the saved list
//! keeps a placeholder in their place. The `remote` bridge, which runs as the agents' user, gets a grant URL and no
//! headers; the keeper refreshes the sign-in itself, and drops it once it can't (`sign_in_ended`). The sign-in still
//! runs in the host, which hands the tokens over as soon as it has them.
//!
//! Left readable by agents: a local connector's `env` (its process gets it anyway), the connector's URL (a token in
//! its query or user part too), 1Password references (resolved at each use, like the 1Password token they need), the
//! sign-in's client registration, and the headers of a connector on plain `http` (the keeper sends secrets over https
//! only).

use super::{Connector, oauth::OAuth, passwords};
use crate::keeper::PLACEHOLDER;
use crate::keeper::client;
use crate::keeper::record::{self, Keep, Record};
use crate::store::Store;
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

fn slot(id: &str) -> String {
    format!("connector:{id}")
}

/// A header value only the keeper may hold: not handed over yet, and not a 1Password reference.
fn literal(value: &str) -> bool {
    value != PLACEHOLDER && !value.starts_with("op://")
}

/// Where a connector's secrets may go: its origin, when it is on https (a server on the older SSE transport names
/// its message path from the origin's root).
fn origin(url: &str) -> Option<String> {
    let url = reqwest::Url::parse(url).ok()?;
    let origin = url.origin().ascii_serialization();
    record::upstream(&origin).is_ok().then_some(origin)
}

/// Hands the secrets of `items`, about to be saved, to the keeper, leaving placeholders; drops the keeper's records
/// of connectors that are gone.
pub fn hand_over(store: &Store, items: &mut [Connector]) -> Result<()> {
    let mut live = BTreeSet::new();
    for c in items.iter_mut() {
        let Some((url, upstream)) = c.url.clone().and_then(|url| origin(&url).map(|o| (url, o))) else { continue };
        live.insert(slot(&c.id));
        let headers: BTreeMap<String, String> =
            c.headers.iter().filter(|(_, v)| literal(v)).map(|(k, v)| (k.to_ascii_lowercase(), v.clone())).collect();
        let token: Option<OAuth> = c.oauth.clone().filter(|o| o.signed_in() && o.access_token != PLACEHOLDER);
        if headers.is_empty() && token.is_none() {
            continue;
        }
        let mut record = Record::new(&upstream, headers);
        if let Some(o) = &token {
            record.headers.insert("authorization".into(), format!("Bearer {}", o.access_token));
            record.oauth = Some(record::OAuth {
                token_url: o.token_endpoint.clone(),
                client_id: o.client_id.clone(),
                client_secret: o.client_secret.clone(),
                refresh_token: o.refresh_token.clone(),
                expires_at: o.expires_at.unwrap_or(0),
                header: "authorization".into(),
                resource: Some(url.clone()),
            });
        }
        let keep = Keep {
            headers: c.headers.iter().filter(|(_, v)| *v == PLACEHOLDER).map(|(k, _)| k.clone()).collect(),
            oauth: token.is_none() && c.oauth.as_ref().is_some_and(|o| o.access_token == PLACEHOLDER),
        };
        client::put_proxied(store, &slot(&c.id), &record, &keep)?;
        for value in c.headers.values_mut().filter(|v| literal(v)) {
            PLACEHOLDER.clone_into(value);
        }
        if let Some(o) = c.oauth.as_mut().filter(|_| token.is_some()) {
            PLACEHOLDER.clone_into(&mut o.access_token);
            o.refresh_token = None;
            o.expires_at = None;
        }
    }
    for gone in client::names(store, "connector:")?.into_iter().filter(|s| !live.contains(s)) {
        client::delete(store, &gone)?;
    }
    Ok(())
}

/// Whether the keeper dropped `c`'s sign-in, which it does once it can't renew it (no refresh token, or the server
/// refused): the host then signs the connector out, so the apps offer to sign in again.
pub fn sign_in_ended(store: &Store, c: &Connector) -> Result<bool> {
    let with_keeper = c.oauth.as_ref().is_some_and(|o| o.access_token == PLACEHOLDER);
    Ok(with_keeper && !client::signed_in(store, &slot(&c.id))?)
}

/// What the `remote` bridge sends connector `c` (at `url`) to: a grant with the connector's path, and `base`, the
/// grant itself, for the paths a server on the older SSE transport names. `None`: the keeper has nothing for it.
pub async fn target(store: &Store, c: &Connector, url: &str) -> Result<Option<Value>> {
    let Some(upstream) = origin(url) else { return Ok(None) };
    let slot = slot(&c.id);
    // 1Password references resolve at each use, as on any computer; the keeper sends what they hold.
    let mut resolved = BTreeMap::new();
    for (name, value) in c.headers.iter().filter(|(_, v)| v.starts_with("op://")) {
        resolved.insert(name.to_ascii_lowercase(), passwords::resolve(store, value).await?);
    }
    if !resolved.is_empty() {
        let record = Record::new(&upstream, resolved);
        let keep = Keep { headers: c.headers.keys().cloned().collect(), oauth: true };
        client::put_proxied(store, &slot, &record, &keep)?;
    }
    if !client::has(store, &slot)? {
        return Ok(None);
    }
    let base = client::short_grant(store, &slot)?;
    let parsed = reqwest::Url::parse(url)?;
    let query = parsed.query().map(|q| format!("?{q}")).unwrap_or_default();
    Ok(Some(json!({"url": format!("{base}{}{query}", parsed.path()), "headers": {}, "base": base})))
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::market::{connectors, save_connectors, update_connectors};

    #[tokio::test(flavor = "multi_thread")]
    async fn a_remote_connectors_secrets_stay_with_the_keeper() {
        let k = crate::keeper::tests::test_keeper();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        k.manage(&store);
        let connector: Connector = serde_json::from_value(json!({
            "id": "linear", "name": "Linear", "url": "https://mcp.example.com/sse?team=1",
            "headers": {"X-Team": "t1", "X-Ref": "op://vault/item/field"},
            "oauth": {"clientId": "c", "tokenEndpoint": "https://auth.example.com/token", "accessToken": "at-1",
                      "refreshToken": "rt-1", "expiresAt": 99},
        }))
        .unwrap();
        let local: Connector =
            serde_json::from_value(json!({"id": "fs", "name": "Files", "command": "npx", "env": {"TOKEN": "local"}}))
                .unwrap();
        save_connectors(&store, &[connector, local]).unwrap();

        let saved = client::reveal(&store, "connectors").unwrap().unwrap();
        assert!(!saved.contains("at-1") && !saved.contains("rt-1") && !saved.contains("\"t1\""), "{saved}");
        assert!(saved.contains("op://vault/item/field") && saved.contains("local"), "references and local env stay");
        let listed = connectors(&store).unwrap();
        assert_eq!(listed[0].public()["auth"], "signedIn");
        let record = k.keeper.record("connector:linear").unwrap().unwrap();
        assert_eq!(record.upstream, "https://mcp.example.com", "the origin: older servers name paths from its root");
        assert_eq!(record.headers["x-team"], "t1");
        assert_eq!(record.headers["authorization"], "Bearer at-1");
        let oauth = record.oauth.clone().unwrap();
        assert_eq!((oauth.refresh_token.as_deref(), oauth.expires_at), (Some("rt-1"), 99));
        assert!(!client::has(&store, "connector:fs").unwrap());

        // Changing one header keeps the rest and the sign-in, which only the keeper has now.
        update_connectors(&store, |all| {
            all[0].headers.insert("X-Team".into(), "t2".into());
            Ok(())
        })
        .unwrap();
        let record = k.keeper.record("connector:linear").unwrap().unwrap();
        assert_eq!(record.headers["x-team"], "t2");
        assert_eq!(record.headers["authorization"], "Bearer at-1");
        assert!(record.oauth.is_some());

        // The bridge gets a grant under the connector's path, and no headers.
        let target = crate::market::oauth::target(&store, "linear", false).await;
        // The 1Password reference can't resolve here (no 1Password), which stops it before any grant.
        assert!(target.is_err());
        update_connectors(&store, |all| {
            all[0].headers.remove("X-Ref");
            Ok(())
        })
        .unwrap();
        let target = crate::market::oauth::target(&store, "linear", false).await.unwrap();
        let base = target["base"].as_str().unwrap();
        assert!(base.starts_with(&format!("http://127.0.0.1:{}/t/", k.keeper.port)));
        assert_eq!(target["url"], format!("{base}/sse?team=1"));
        assert_eq!(target["headers"], json!({}));

        // The keeper ended the sign-in (it couldn't renew it): the next 401 signs the connector out, as anywhere else.
        let mut ended = k.keeper.record("connector:linear").unwrap().unwrap();
        ended.oauth = None;
        ended.headers.remove("authorization");
        k.keeper.put_record("connector:linear", &ended).unwrap();
        assert!(crate::market::oauth::target(&store, "linear", false).await.is_ok(), "only a 401 asks");
        let refused = crate::market::oauth::target(&store, "linear", true).await.unwrap_err().to_string();
        assert!(refused.contains("Sign in to Linear"), "{refused}");
        assert_eq!(connectors(&store).unwrap()[0].public()["auth"], "signedOut");
        assert!(crate::market::oauth::target(&store, "linear", false).await.is_err(), "until signed in again");

        // A removed connector's record goes too.
        update_connectors(&store, |all| {
            all.retain(|c| c.id != "linear");
            Ok(())
        })
        .unwrap();
        assert!(!client::has(&store, "connector:linear").unwrap());
    }
}
