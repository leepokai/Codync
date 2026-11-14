//! On a Codync Cloud computer, the provider keys saved for an agent (slot `agent-env:<backend>`) or in an API key
//! login profile (`login-profile:<id>`) that can go through a base URL are its keeper's (`crate::keeper`): proxied
//! slots `<slot>:<VAR>`. The agent starts with `VAR=codync-keeper` and the provider's base-URL variable set to a
//! grant, so it can call the provider but never read the key. A key saved with its base-URL variable stays visible:
//! it goes to that address as it is. Every other variable stays in the visible map `<slot>`, which agents can read.

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

/// Where `slot`'s proxied key `var` is.
fn proxied_slot(slot: &str, var: &str) -> String {
    format!("{slot}:{var}")
}

/// The proxied keys saved in `slot`.
pub fn names(store: &Store, slot: &str) -> Result<Vec<String>> {
    let prefix = proxied_slot(slot, "");
    Ok(client::names(store, &prefix)?.into_iter().filter_map(|s| s.strip_prefix(&prefix).map(str::to_owned)).collect())
}

/// The variable that holds the address key `var` is sent to, when the keeper can stand in for it.
pub fn address_of(var: &str) -> Option<&'static str> {
    KEYS.iter().find(|k| k.var == var).map(|k| k.base)
}

/// Deletes `slot`'s proxied keys.
pub fn forget(store: &Store, slot: &str) -> Result<()> {
    names(store, slot)?.iter().try_for_each(|var| client::delete(store, &proxied_slot(slot, var)))
}

/// Adds `slot`'s proxied keys (but the ones `skip` names) to its process environment: each as the placeholder,
/// with its base-URL variable on a fresh grant (in `grants`, which the process holds until it ends). Two keys of one
/// provider share its base URL: the first one's grant serves both (the keeper drops whatever key header the agent
/// sends).
pub fn add(
    store: &Store,
    slot: &str,
    skip: &[&str],
    env: &mut Vec<(String, String)>,
    grants: &mut Grants,
) -> Result<()> {
    let saved = names(store, slot)?;
    for key in KEYS.iter().filter(|k| saved.iter().any(|s| s == k.var) && !skip.contains(&k.var)) {
        env.push((key.var.into(), PLACEHOLDER.into()));
        if !env.iter().any(|(name, _)| name == key.base) {
            env.push((key.base.into(), grants.grant(store, &proxied_slot(slot, key.var))?));
        }
    }
    Ok(())
}

/// Applies `changes` (a value, or `None` to remove it) to `slot`'s keys: proxyable keys go to the keeper unless
/// the result also sets their base-URL variable; the rest is saved in the visible map.
pub fn save(store: &Store, slot: &str, changes: &[(String, Option<String>)]) -> Result<()> {
    let proxied: BTreeSet<String> = names(store, slot)?.into_iter().collect();
    let mut visible = super::visible(store, slot)?;
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
        client::put_proxied(store, &proxied_slot(slot, key.var), &key.record(&value), &Keep::default())?;
    }
    crate::market::vault::write(store, slot, &serde_json::to_string(&visible)?)?;
    for var in deletes {
        client::delete(store, &proxied_slot(slot, var))?;
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::super::{env_slot, env_without, saved_env};
    use super::*;
    use std::collections::BTreeMap;

    fn change(name: &str, value: Option<&str>) -> (String, Option<String>) {
        (name.to_owned(), value.map(str::to_owned))
    }

    fn save_now(store: &Store, changes: &[(String, Option<String>)]) -> Result<()> {
        save(store, &env_slot("claude"), changes)
    }

    /// The keys the keeper holds for Claude Code.
    fn proxied(store: &Store) -> Vec<String> {
        names(store, &env_slot("claude")).unwrap()
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
        assert_eq!(proxied(&store), ["ANTHROPIC_API_KEY"]);
        assert!(client::reveal(&store, "agent-env:claude:ANTHROPIC_API_KEY").is_err());
        assert_eq!(
            saved_env(&store, "claude").unwrap(),
            ["ANTHROPIC_API_KEY", "CODEX_HOME", "OPENAI_API_KEY", "OPENAI_BASE_URL"]
        );

        let (vars, grants) = env_without(&store, &env_slot("claude"), &[]).unwrap();
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

        // On a login profile the agent's login keys stay out, with their address and its grant.
        crate::agent::login_profiles::tests::insert(&store, &[("a1", "claude")]);
        let (vars, grants) = super::super::launch_env(&store, "claude", Some("a1")).unwrap();
        let env: BTreeMap<String, String> = vars.into_iter().collect();
        assert!(!env.contains_key("ANTHROPIC_API_KEY") && !env.contains_key("ANTHROPIC_BASE_URL"), "{env:?}");
        assert!(grants.is_empty() && env.contains_key("CLAUDE_CONFIG_DIR"));
        assert_eq!(env["OPENAI_API_KEY"], "sk-oai", "not a key Claude Code signs in with");

        // A base URL for a key only the keeper has: the key has to be entered again.
        let refused = save_now(&store, &[change("ANTHROPIC_BASE_URL", Some("https://proxy.example"))]).unwrap_err();
        assert!(refused.to_string().contains("Enter ANTHROPIC_API_KEY again"));
        assert!(
            !client::reveal(&store, "agent-env:claude").unwrap().unwrap().contains("proxy.example"),
            "nothing saved"
        );

        // Dropping its own address moves a visible key to the keeper; removing a key removes its slot.
        save_now(&store, &[change("OPENAI_BASE_URL", None), change("ANTHROPIC_API_KEY", None)]).unwrap();
        assert_eq!(proxied(&store), ["OPENAI_API_KEY"]);
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
        assert_eq!(proxied(&store), Vec::<String>::new());
        assert!(client::reveal(&store, "agent-env:claude").unwrap().unwrap().contains("sk-oai-2"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_agent_holds_its_grants_until_it_is_stopped() {
        let k = crate::keeper::tests::test_keeper();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        k.manage(&store);
        save_now(&store, &[change("GEMINI_API_KEY", Some("g"))]).unwrap();
        let (vars, grants) = env_without(&store, &env_slot("claude"), &[]).unwrap();
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

    #[tokio::test(flavor = "multi_thread")]
    async fn an_api_key_login_profiles_provider_keys_go_to_the_keeper_too() {
        use crate::agent::login_profiles::{self, Kind};
        let k = crate::keeper::tests::test_keeper();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        k.manage(&store);
        let added = login_profiles::add(&store, "claude", Kind::ApiKey).await.unwrap();
        let id = added["id"].as_str().unwrap();
        let typed = serde_json::json!({"ANTHROPIC_API_KEY": "sk-ant", "MY_FLAG": "1"});
        let set = login_profiles::keys::set(&store, id, typed.as_object().unwrap(), &[]).await.unwrap();
        assert_eq!(set.vars, ["ANTHROPIC_API_KEY", "MY_FLAG"]);
        let slot = format!("login-profile:{id}");
        assert!(!client::reveal(&store, &slot).unwrap().unwrap().contains("sk-ant"), "agents can't read it");

        let (vars, grants) = super::super::launch_env(&store, "claude", Some(id)).unwrap();
        let env: BTreeMap<String, String> = vars.into_iter().collect();
        assert_eq!((env["ANTHROPIC_API_KEY"].as_str(), env["MY_FLAG"].as_str()), (PLACEHOLDER, "1"));
        assert!(!grants.is_empty() && env.contains_key("ANTHROPIC_BASE_URL"));
        assert!(super::super::launch_sets(&store, "claude", Some(id), "ANTHROPIC_API_KEY"));
        assert!(!super::super::launch_sets(&store, "claude", None, "ANTHROPIC_API_KEY"));

        super::super::forget_env(&store, &slot).unwrap();
        assert!(names(&store, &slot).unwrap().is_empty() && !crate::market::vault::saved(&store, &slot).unwrap());
    }

    #[test]
    fn a_computer_of_the_users_own_hands_its_keys_over_as_they_are() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let saved = serde_json::json!({"ANTHROPIC_API_KEY": "sk-ant", "OTHER": "o"});
        crate::market::vault::write(&store, &env_slot("claude"), &saved.to_string()).unwrap();
        let (env, grants) = env_without(&store, &env_slot("claude"), &[]).unwrap();
        assert!(grants.is_empty(), "no grants off a cloud computer");
        assert_eq!(env, [("ANTHROPIC_API_KEY".to_owned(), "sk-ant".to_owned()), ("OTHER".to_owned(), "o".to_owned())]);
        crate::agent::login_profiles::tests::insert(&store, &[("a1", "claude")]);
        let (env, _) = super::super::launch_env(&store, "claude", Some("a1")).unwrap();
        let names: Vec<&str> = env.iter().map(|(name, _)| name.as_str()).collect();
        assert!(
            names.contains(&"OTHER") && !names.contains(&"ANTHROPIC_API_KEY"),
            "the login profile's login: {names:?}"
        );
        assert_eq!(saved_env(&store, "claude").unwrap(), ["ANTHROPIC_API_KEY", "OTHER"]);
        // What tells whether the process runs on a key: the saved one, unless a login profile leaves it out.
        let sets = |profile| super::super::launch_sets(&store, "claude", profile, "ANTHROPIC_API_KEY");
        assert!(sets(None) && !sets(Some("a1")));
    }
}
