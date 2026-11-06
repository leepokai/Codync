//! Browsing the MCP Registry: featured servers, cached first page, searches and listings.

use super::install::install_options;
use super::{Connector, MCP_REGISTRY, SEARCH_TIMEOUT, connectors, get_json, get_json_within};
use crate::LockExt;
use anyhow::{Context, Result, anyhow};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Well-known connectors shown before you search, by MCP Registry name.
/// Names that aren't in the registry (yet) are simply skipped.
const FEATURED: &[&str] = &[
    "io.github.ComposioHQ/composio",
    "io.github.github/github-mcp-server",
    "app.linear/linear",
    "com.notion/mcp",
    "com.figma.mcp/mcp",
    "com.stripe/mcp",
    "com.vercel/vercel-mcp",
    "com.supabase/mcp",
    "com.cloudflare.mcp/mcp",
    "com.atlassian/atlassian-mcp-server",
    "io.github.upstash/context7",
    "io.github.microsoft/playwright-mcp",
];

/// One page of the registry (a search, or everything), with the cursor for the next page.
async fn registry_page(search: &str, cursor: &str, limit: usize) -> Result<(Vec<Value>, Option<String>)> {
    let mut url = reqwest::Url::parse(MCP_REGISTRY)?;
    url.query_pairs_mut().append_pair("version", "latest").append_pair("limit", &limit.to_string());
    if !search.is_empty() {
        url.query_pairs_mut().append_pair("search", search);
    }
    if !cursor.is_empty() {
        url.query_pairs_mut().append_pair("cursor", cursor);
    }
    let v = get_json_within(url.as_str(), SEARCH_TIMEOUT).await.context("searching the MCP Registry")?;
    let servers =
        v["servers"].as_array().cloned().unwrap_or_default().into_iter().map(|s| s["server"].clone()).collect();
    Ok((servers, v["metadata"]["nextCursor"].as_str().filter(|c| !c.is_empty()).map(str::to_owned)))
}

/// One registry server by exact name (its latest version).
pub(super) async fn registry_server(name: &str) -> Result<Value> {
    let mut url = reqwest::Url::parse(MCP_REGISTRY)?;
    url.path_segments_mut().map_err(|()| anyhow!("bad registry URL"))?.extend([name, "versions", "latest"]);
    let v = get_json(url.as_str()).await.with_context(|| format!("{name} isn't in the MCP Registry"))?;
    Ok(v["server"].clone())
}

/// "com.notion/mcp" -> "Notion", "io.github.microsoft/playwright-mcp" -> "Playwright".
fn title_from_name(name: &str) -> String {
    let (org, last) = name.split_once('/').unwrap_or(("", name));
    let words: Vec<&str> =
        last.split(['-', '_', '.']).filter(|w| !w.is_empty() && !matches!(*w, "mcp" | "server")).collect();
    let words = if words.is_empty() {
        let label = org
            .split('.')
            .find(|l| !matches!(*l, "com" | "io" | "app" | "ai" | "dev" | "org" | "github"))
            .unwrap_or(org);
        vec![label]
    } else {
        words
    };
    words
        .iter()
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn listing(server: &Value, installed: &[Connector]) -> Option<Value> {
    let name = server["name"].as_str()?;
    let options = install_options(server);
    if options.is_empty() {
        return None;
    }
    let title = server["title"].as_str().map_or_else(|| title_from_name(name), str::to_owned);
    Some(json!({
        "name": name,
        "title": title,
        "description": server["description"],
        "version": server["version"],
        "website": server["websiteUrl"].as_str().or_else(|| server["repository"]["url"].as_str()),
        "installed": installed.iter().any(|c| c.registry_name.as_deref() == Some(name)),
        "options": options,
    }))
}

/// Browse the whole MCP Registry a page at a time (featured connectors lead the first
/// page of the unfiltered list), or search it. `nextCursor` fetches the following page.
pub async fn browse_connectors(store: &crate::store::Store, search: &str, cursor: &str) -> Result<Value> {
    let deadline = Instant::now() + BROWSE_BUDGET;
    let installed = connectors(store)?;
    let search = search.trim();
    let (servers, next) =
        if search.is_empty() && cursor.is_empty() { first_page().await? } else { later_page(search, cursor).await? };
    let (items, next) = past_empty_pages((listings(&servers, &installed), next), cursor, deadline, |after| {
        let installed = &installed;
        async move {
            let (servers, next) = later_page(search, &after).await?;
            Ok((listings(&servers, installed), next))
        }
    })
    .await?;
    Ok(json!({"items": items, "nextCursor": next}))
}

/// The servers on a page that Codync can install, each once.
fn listings(servers: &[Value], installed: &[Connector]) -> Vec<Value> {
    let mut seen = std::collections::HashSet::new();
    servers
        .iter()
        .filter(|s| seen.insert(s["name"].as_str().unwrap_or_default().to_owned()))
        .filter_map(|s| listing(s, installed))
        .collect()
}

/// Registry pages read past one with nothing Codync can install, within `BROWSE_BUDGET`, so an app
/// gets rows in fewer round trips. An app that still gets an empty page with a cursor reads on by itself.
const EMPTY_PAGE_SKIPS: usize = 3;
/// The apps wait 60 s for `marketConnectors`, and one registry page can take 45 s: skipping stops
/// in time to answer, handing back the cursor it didn't get to.
const BROWSE_BUDGET: Duration = Duration::from_secs(40);

async fn past_empty_pages<F, Fut>(page: Page, cursor: &str, deadline: Instant, mut fetch: F) -> Result<Page>
where
    F: FnMut(String) -> Fut,
    Fut: Future<Output = Result<Page>>,
{
    let (mut items, mut next) = page;
    let mut asked = cursor.to_owned();
    for _ in 0..EMPTY_PAGE_SKIPS {
        if !items.is_empty() {
            break;
        }
        let Some(after) = following(&asked, next.take()) else { break };
        let Some(left) = deadline.checked_duration_since(Instant::now()) else { return Ok((items, Some(after))) };
        let Ok(page) = tokio::time::timeout(left, fetch(after.clone())).await else { return Ok((items, Some(after))) };
        (items, next) = page?;
        asked = after;
    }
    Ok((items, following(&asked, next)))
}

/// The registry handing back the cursor it was asked with would page the same rows forever: that ends the list.
fn following(cursor: &str, next: Option<String>) -> Option<String> {
    next.filter(|n| n != cursor)
}

type Page = (Vec<Value>, Option<String>);

/// The unfiltered first page (featured servers, then the registry's first page), kept in memory
/// because the registry often takes 15-45 s to answer.
static FIRST_PAGE: Mutex<Option<(Instant, Page)>> = Mutex::new(None);
const FIRST_PAGE_FRESH: Duration = Duration::from_mins(10);

/// Serves the cached first page at once (refreshing it in the background when stale).
async fn first_page() -> Result<Page> {
    let cached = FIRST_PAGE.locked().clone();
    match cached {
        Some((at, page)) => {
            if at.elapsed() > FIRST_PAGE_FRESH {
                tokio::spawn(refresh_first_page());
            }
            Ok(page)
        }
        None => fetch_first_page().await,
    }
}

/// Fetches the first page into the cache; the host runs it at launch so the Marketplace opens warm.
pub async fn refresh_first_page() {
    if let Err(e) = fetch_first_page().await {
        tracing::debug!(error = format!("{e:#}"), "MCP Registry first page");
    }
}

async fn fetch_first_page() -> Result<Page> {
    let lookups = FEATURED.iter().map(|n| registry_server(n));
    let (featured, page) = tokio::join!(futures::future::join_all(lookups), registry_page("", "", 60));
    let mut servers: Vec<Value> = featured.into_iter().filter_map(Result::ok).collect();
    let complete = servers.len() == FEATURED.len();
    let next = match page {
        Ok((page, next)) => {
            servers.extend(page);
            next
        }
        // The featured servers alone still beat an error; nothing is cached so the next open retries.
        Err(e) if !servers.is_empty() => {
            tracing::debug!(error = format!("{e:#}"), "MCP Registry first page");
            return Ok((servers, None));
        }
        Err(e) => return Err(e),
    };
    if complete {
        *FIRST_PAGE.locked() = Some((Instant::now(), (servers.clone(), next.clone())));
    }
    Ok((servers, next))
}

/// A search, or a later page of the unfiltered list (which skips the featured servers the first page led with).
async fn later_page(search: &str, cursor: &str) -> Result<Page> {
    let (page, next) = registry_page(search, cursor, 60).await?;
    let shown_already = |s: &Value| search.is_empty() && FEATURED.contains(&s["name"].as_str().unwrap_or_default());
    Ok((page.into_iter().filter(|s| !shown_already(s)).collect(), next))
}

pub async fn connector_info(store: &crate::store::Store, name: &str) -> Result<Value> {
    let server = registry_server(name).await?;
    listing(&server, &connectors(store)?).ok_or_else(|| anyhow!("No supported installation option"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_come_from_names() {
        assert_eq!(title_from_name("com.notion/mcp"), "Notion");
        assert_eq!(title_from_name("com.vercel/vercel-mcp"), "Vercel");
        assert_eq!(title_from_name("io.github.microsoft/playwright-mcp"), "Playwright");
    }

    #[tokio::test]
    async fn an_empty_page_with_a_cursor_reads_on() {
        let pages = |cursor: String| async move {
            Ok(match cursor.as_str() {
                "b" => (vec![], Some("c".to_owned())),
                "c" => (vec![json!({"name": "found"})], Some("d".to_owned())),
                _ => (vec![], Some(cursor)),
            })
        };
        let later = Instant::now() + Duration::from_mins(1);
        let (items, next) = past_empty_pages((vec![], Some("b".into())), "", later, pages).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(next.as_deref(), Some("d"));
        let (items, next) = past_empty_pages((vec![], Some("x".into())), "", later, pages).await.unwrap();
        assert!(items.is_empty() && next.is_none(), "a cursor the registry repeats still ends the list");
        let (items, next) = past_empty_pages((vec![json!({})], Some("b".into())), "", later, pages).await.unwrap();
        assert_eq!((items.len(), next.as_deref()), (1, Some("b")), "a page with rows is kept as it is");
        let slow = |_: String| async {
            tokio::time::sleep(Duration::from_secs(5)).await;
            Ok((vec![json!({})], None))
        };
        let soon = Instant::now() + Duration::from_millis(20);
        let (items, next) = past_empty_pages((vec![], Some("b".into())), "", soon, slow).await.unwrap();
        assert!(items.is_empty() && next.as_deref() == Some("b"), "past the deadline the cursor comes back unread");
    }

    #[test]
    fn a_repeated_cursor_ends_the_list() {
        assert_eq!(following("a", Some("a".into())), None);
        assert_eq!(following("a", Some("b".into())), Some("b".into()));
        assert_eq!(following("", None), None);
    }
}
