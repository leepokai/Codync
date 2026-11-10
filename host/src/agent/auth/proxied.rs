//! On a Codync Cloud computer, an agent's provider keys that can go through a base URL are its keeper's
//! (`crate::keeper`): proxied slots `agent-env:<backend>:<VAR>`. The agent starts with `VAR=codync-keeper` and the
//! provider's base-URL variable set to a grant, so it can call the provider but never read the key. A key saved
//! with its base-URL variable stays visible: it goes to that address as it is. Every other variable stays in the
//! visible map `agent-env:<backend>`, which agents can read.

use crate::keeper::PLACEHOLDER;
use crate::keeper::client::{self, Grants};
use crate::keeper::record::{ANTHROPIC, GEMINI, Keep, Record};
use crate::store::Store;
use anyhow::{Result, bail};
use std::collections::BTreeSet;

/// How a provider takes its key.
enum Auth {
    Header(&'static str),
    Bearer,
    /// `x-goog-api-key` (Google's SDKs may also send it as `?key=`).
    GoogleKey,
}

/// A key the keeper can stand in for, and where it goes.
struct Proxyable {
    var: &'static str,
    /// The variable that points the agent at the provider.
    base: &'static str,
    upstream: &'static str,
    auth: Auth,
}

#[rustfmt::skip]
const KEYS: [Proxyable; 5] = [
    Proxyable { var: "ANTHROPIC_API_KEY", base: "ANTHROPIC_BASE_URL", upstream: ANTHROPIC, auth: Auth::Header("x-api-key") },
    Proxyable { var: "ANTHROPIC_AUTH_TOKEN", base: "ANTHROPIC_BASE_URL", upstream: ANTHROPIC, auth: Auth::Bearer },
    Proxyable { var: "OPENAI_API_KEY", base: "OPENAI_BASE_URL", upstream: "https://api.openai.com/v1", auth: Auth::Bearer },
    Proxyable { var: "GEMINI_API_KEY", base: "GOOGLE_GEMINI_BASE_URL", upstream: GEMINI, auth: Auth::GoogleKey },
    Proxyable { var: "GOOGLE_API_KEY", base: "GOOGLE_GEMINI_BASE_URL", upstream: GEMINI, auth: Auth::GoogleKey },
];

impl Proxyable {
    fn record(&self, key: &str) -> Record {
        match self.auth {
            Auth::Header(name) => Record::header(self.upstream, name, key.to_owned()),
            Auth::Bearer => Record::bearer(self.upstream, key),
            Auth::GoogleKey => Record::google_key(self.upstream, key),
        }
    }
}

fn slot(backend: &str, var: &str) -> String {
    format!("agent-env:{backend}:{var}")
}

/// The proxied keys saved for `backend`.
pub fn names(store: &Store, backend: &str) -> Result<Vec<String>> {
    let prefix = slot(backend, "");
    Ok(client::names(store, &prefix)?.into_iter().filter_map(|s| s.strip_prefix(&prefix).map(str::to_owned)).collect())
}

/// Adds `backend`'s proxied keys to its process environment: each as the placeholder, with its base-URL variable on
/// a fresh grant (in `grants`, which the process holds until it ends). Two keys of one provider share its base URL:
/// the first one's grant serves both (the keeper drops whatever key header the agent sends).
pub fn add(store: &Store, backend: &str, env: &mut Vec<(String, String)>, grants: &mut Grants) -> Result<()> {
    let saved = names(store, backend)?;
    for key in KEYS.iter().filter(|k| saved.iter().any(|s| s == k.var)) {
        env.push((key.var.into(), PLACEHOLDER.into()));
        if !env.iter().any(|(name, _)| name == key.base) {
            env.push((key.base.into(), grants.grant(store, &slot(backend, key.var))?));
        }
    }
    Ok(())
}

/// Applies `changes` (a value, or `None` to remove it) to `backend`'s keys: proxyable keys go to the keeper unless
/// the result also sets their base-URL variable; the rest is saved in the visible map.
pub fn save(store: &Store, backend: &str, changes: &[(String, Option<String>)]) -> Result<()> {
    let proxied: BTreeSet<String> = names(store, backend)?.into_iter().collect();
    let mut visible = super::visible(store, backend)?;
    let removed = super::apply(&mut visible, changes);
    // `visible` holds every value known now; the proxied keys move out of it.
    let mut puts = Vec::new();
    let mut deletes: Vec<&str> = removed.iter().map(String::as_str).filter(|n| proxied.contains(*n)).collect();
    for key in &KEYS {
        let own_address = visible.contains_key(key.base);
        match visible.get(key.var) {
            Some(value) if !own_address => {
                puts.push((key, value.clone()));
                visible.remove(key.var);
            }
            Some(_) if proxied.contains(key.var) => deletes.push(key.var),
            None if own_address && proxied.contains(key.var) && !removed.contains(key.var) => {
                bail!("Enter {} again along with {}: a key for your own address is saved as it is.", key.var, key.base)
            }
            _ => {}
        }
    }
    for (key, value) in puts {
        client::put_proxied(store, &slot(backend, key.var), &key.record(&value), &Keep::default())?;
    }
    crate::market::vault::write(store, &super::env_slot(backend), &serde_json::to_string(&visible)?)?;
    for var in deletes {
        client::delete(store, &slot(backend, var))?;
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::super::{env, env_slot, saved_env};
    use super::*;
    use std::collections::BTreeMap;

    fn change(name: &str, value: Option<&str>) -> (String, Option<String>) {
        (name.to_owned(), value.map(str::to_owned))
    }

    fn save_now(store: &Store, changes: &[(String, Option<String>)]) -> Result<()> {
        save(store, "claude", changes)
    }

    #[test]
    fn provider_keys_go_to_the_keeper_unless_they_have_their_own_address() {
        let k = crate::keeper::tests::test_keeper();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        k.manage(&store);
        save_now(
            &store,
            &[
                change("ANTHROPIC_API_KEY", Some("sk-ant")),
                change("OPENAI_API_KEY", Some("sk-oai")),
                change("OPENAI_BASE_URL", Some("https://my.example/v1")),
                change("CODEX_HOME", Some("/w")),
            ],
        )
        .unwrap();
        // Readable by agents: everything but the Anthropic key, which has no address of its own.
        let readable = client::reveal(&store, "agent-env:claude").unwrap().unwrap();
        assert!(!readable.contains("sk-ant") && readable.contains("sk-oai"), "{readable}");
        assert_eq!(names(&store, "claude").unwrap(), ["ANTHROPIC_API_KEY"]);
        assert!(client::reveal(&store, "agent-env:claude:ANTHROPIC_API_KEY").is_err());
        assert_eq!(
            saved_env(&store, "claude").unwrap(),
            ["ANTHROPIC_API_KEY", "CODEX_HOME", "OPENAI_API_KEY", "OPENAI_BASE_URL"]
        );

        let (vars, grants) = env(&store, "claude").unwrap();
        let env: BTreeMap<String, String> = vars.into_iter().collect();
        assert_eq!(env["ANTHROPIC_API_KEY"], PLACEHOLDER);
        let grant = format!("http://127.0.0.1:{}/t/", k.keeper.port);
        assert!(env["ANTHROPIC_BASE_URL"].starts_with(&grant));
        assert_eq!(env["OPENAI_API_KEY"], "sk-oai");
        assert_eq!(env["OPENAI_BASE_URL"], "https://my.example/v1");
        let record = k.keeper.record("agent-env:claude:ANTHROPIC_API_KEY").unwrap().unwrap();
        assert_eq!((record.upstream.as_str(), record.headers["x-api-key"].as_str()), (ANTHROPIC, "sk-ant"));
        // The grant lasts as long as the process holding it: revoked once it stops.
        let token = env["ANTHROPIC_BASE_URL"].rsplit('/').next().unwrap().to_owned();
        assert!(k.keeper.granted(&token).is_some());
        drop(grants);
        assert!(k.keeper.granted(&token).is_none(), "nobody holds it");

        // A base URL for a key only the keeper has: the key has to be entered again.
        let refused = save_now(&store, &[change("ANTHROPIC_BASE_URL", Some("https://proxy.example"))]).unwrap_err();
        assert!(refused.to_string().contains("Enter ANTHROPIC_API_KEY again"));
        assert!(
            !client::reveal(&store, "agent-env:claude").unwrap().unwrap().contains("proxy.example"),
            "nothing saved"
        );

        // Dropping its own address moves a visible key to the keeper; removing a key removes its slot.
        save_now(&store, &[change("OPENAI_BASE_URL", None), change("ANTHROPIC_API_KEY", None)]).unwrap();
        assert_eq!(names(&store, "claude").unwrap(), ["OPENAI_API_KEY"]);
        let readable = client::reveal(&store, "agent-env:claude").unwrap().unwrap();
        assert!(!readable.contains("sk-oai"), "{readable}");
        let record = k.keeper.record("agent-env:claude:OPENAI_API_KEY").unwrap().unwrap();
        assert_eq!(record.headers["authorization"], "Bearer sk-oai");

        // With its address again, entered along with it, the key is visible once more.
        save_now(
            &store,
            &[change("OPENAI_API_KEY", Some("sk-oai-2")), change("OPENAI_BASE_URL", Some("https://my.example/v1"))],
        )
        .unwrap();
        assert_eq!(names(&store, "claude").unwrap(), Vec::<String>::new());
        assert!(client::reveal(&store, "agent-env:claude").unwrap().unwrap().contains("sk-oai-2"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_agent_holds_its_grants_until_it_is_stopped() {
        let k = crate::keeper::tests::test_keeper();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        k.manage(&store);
        save_now(&store, &[change("GEMINI_API_KEY", Some("g"))]).unwrap();
        let (vars, grants) = env(&store, "claude").unwrap();
        let base = vars.iter().find(|(k, _)| k == "GOOGLE_GEMINI_BASE_URL").unwrap().1.clone();
        let token = base.rsplit('/').next().unwrap().to_owned();
        let (acp, _inbox) = crate::agent::acp::Acp::spawn("cat", env!("CARGO_MANIFEST_DIR"), &vars).unwrap();
        acp.hold(grants);
        assert!(acp.holds_grants());
        assert!(k.keeper.granted(&token).is_some());
        acp.kill().await;
        assert!(k.keeper.granted(&token).is_none(), "revoked as the agent stops");
        assert!(!acp.holds_grants());
    }

    #[test]
    fn a_computer_of_the_users_own_hands_its_keys_over_as_they_are() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let saved = serde_json::json!({"ANTHROPIC_API_KEY": "sk-ant", "OTHER": "o"});
        crate::market::vault::write(&store, &env_slot("claude"), &saved.to_string()).unwrap();
        let (env, grants) = env(&store, "claude").unwrap();
        assert!(grants.is_empty(), "no grants off a cloud computer");
        assert_eq!(env, [("ANTHROPIC_API_KEY".to_owned(), "sk-ant".to_owned()), ("OTHER".to_owned(), "o".to_owned())]);
        assert_eq!(saved_env(&store, "claude").unwrap(), ["ANTHROPIC_API_KEY", "OTHER"]);
    }
}
