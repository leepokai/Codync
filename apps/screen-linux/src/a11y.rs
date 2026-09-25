//! The active window's accessibility tree over AT-SPI, in the same shape as the
//! macOS helper: `{role, title?, description?, value?, frame: [x, y, w, h], children?}`
//! with frames relative to the display.

use crate::portal::Display;
use anyhow::{Context, Result, anyhow};
use atspi::connection::AccessibilityConnection;
use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::{CoordType, State};
use serde_json::{Map, Value, json};

const MAX_NODES: usize = 600;
const MAX_DEPTH: usize = 30;

pub async fn frontmost(d: &Display) -> Result<Value> {
    let a11y = AccessibilityConnection::new()
        .await
        .context("AT-SPI (accessibility) isn't running")?;
    let conn = a11y.connection();
    let root = a11y.root_accessible_on_registry().await?;
    for app in root.get_children().await? {
        let Ok(app) = app.as_accessible_proxy(conn).await else {
            continue;
        };
        for win in app.get_children().await.unwrap_or_default() {
            let Ok(win) = win.as_accessible_proxy(conn).await else {
                continue;
            };
            if win
                .get_state()
                .await
                .is_ok_and(|s| s.contains(State::Active))
            {
                let mut budget = MAX_NODES;
                let tree = Box::pin(node(conn, &win, 0, &mut budget, d))
                    .await
                    .unwrap_or(Value::Null);
                return Ok(
                    json!({"app": app.name().await.unwrap_or_default(), "tree": tree, "truncated": budget == 0}),
                );
            }
        }
    }
    Err(anyhow!(
        "No window is active, or its app doesn't expose accessibility."
    ))
}

async fn node(
    conn: &zbus::Connection,
    el: &AccessibleProxy<'_>,
    depth: usize,
    budget: &mut usize,
    d: &Display,
) -> Option<Value> {
    if *budget == 0 || depth >= MAX_DEPTH {
        return None;
    }
    *budget -= 1;
    let mut n = Map::new();
    n.insert(
        "role".into(),
        el.get_role_name().await.unwrap_or_default().into(),
    );
    for (key, value) in [
        ("title", el.name().await),
        ("description", el.description().await),
    ] {
        if let Some(v) = value.ok().filter(|v| !v.is_empty()) {
            n.insert(key.into(), v.chars().take(200).collect::<String>().into());
        }
    }
    if let Ok(proxies) = el.proxies().await
        && let Ok(component) = proxies.component().await
        && let Ok((x, y, w, h)) = component.get_extents(CoordType::Screen).await
    {
        n.insert("frame".into(), json!([x - d.x, y - d.y, w, h]));
    }
    let mut children = vec![];
    for child in el.get_children().await.unwrap_or_default() {
        if let Ok(c) = child.as_accessible_proxy(conn).await
            && let Some(v) = Box::pin(node(conn, &c, depth + 1, budget, d)).await
        {
            children.push(v);
        }
    }
    if !children.is_empty() {
        n.insert("children".into(), children.into());
    }
    Some(Value::Object(n))
}
