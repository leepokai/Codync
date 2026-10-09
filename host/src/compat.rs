//! Phone/host compatibility: each side names the oldest version of the other it still works
//! with (docs/reference/compatibility.md). The host sends `MIN_APP` in `hello`; clients keep
//! their own oldest-host floor (`MIN_HOST` here for the terminal client).

use serde_json::Value;

/// The oldest client (iPhone, Mac, Linux app, terminal client) this host still serves correctly.
/// Raise it only after that client version is out (App Store included), and only when this
/// host stops sending or accepting something older clients depend on.
pub const MIN_APP: &str = "2.3.0";

/// The oldest host the terminal client works with.
pub const MIN_HOST: &str = "2.12.0";

/// The iPhone app in the App Store, looked up to hold back host updates that need a newer
/// app than the store has (still in review).
pub const APP_STORE_ID: &str = "6760984418";

/// The iPhone app version live in the App Store; `None` when the lookup has no result.
pub async fn app_store_version() -> anyhow::Result<Option<String>> {
    let url = format!("https://itunes.apple.com/lookup?id={APP_STORE_ID}");
    let res: Value = crate::http()
        .get(url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(res["results"][0]["version"].as_str().map(str::to_owned))
}

/// Whether installing a release that needs `new_min_app` would lock out iPhones.
#[derive(Debug, PartialEq, Eq)]
pub enum Gate {
    Proceed,
    /// The App Store doesn't have that app yet (`store`), or couldn't be asked (`None`).
    WaitForApp {
        required: String,
        store: Option<String>,
    },
}

/// Decides a host update: only a release that raises `minApp`, on a computer with iPhones,
/// waits until the App Store has an app that new. An unanswered lookup waits too: going ahead
/// on a guess is what this gate exists to prevent (a person can still choose to).
pub fn gate(current_min_app: &str, new_min_app: &str, iphones: bool, store: &anyhow::Result<Option<String>>) -> Gate {
    if !below(current_min_app, new_min_app) || !iphones {
        return Gate::Proceed;
    }
    match store {
        Ok(Some(version)) if !below(version, new_min_app) && parse(version).is_some() => Gate::Proceed,
        Ok(Some(version)) => Gate::WaitForApp { required: new_min_app.to_owned(), store: Some(version.clone()) },
        _ => Gate::WaitForApp { required: new_min_app.to_owned(), store: None },
    }
}

/// Which side has to update before a client and a host can work together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mismatch {
    /// The client is older than the host's `minApp`.
    UpdateApp { minimum: String },
    /// The host is older than the client's floor.
    UpdateHost { version: String, minimum: String },
}

/// `major.minor.patch`, ignoring a leading `v` and any pre-release or build suffix; missing
/// parts count as 0. `None` for anything else, which never blocks a connection.
pub fn parse(version: &str) -> Option<(u64, u64, u64)> {
    let core = version.trim().trim_start_matches('v');
    let core = core.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let mut next = |required: bool| match parts.next() {
        Some(p) => p.parse::<u64>().ok(),
        None if required => None,
        None => Some(0),
    };
    let v = (next(true)?, next(false)?, next(false)?);
    parts.next().is_none().then_some(v)
}

/// Whether `version` is below `minimum`; unreadable versions never are.
pub fn below(version: &str, minimum: &str) -> bool {
    matches!((parse(version), parse(minimum)), (Some(v), Some(m)) if v < m)
}

/// Checks a client of version `app` with floor `min_host` against a host's `hello`.
/// Hosts that predate `minApp` only get the floor check.
pub fn check(app: &str, min_host: &str, hello: &Value) -> Option<Mismatch> {
    if let Some(minimum) = hello["minApp"].as_str()
        && below(app, minimum)
    {
        return Some(Mismatch::UpdateApp { minimum: minimum.to_owned() });
    }
    let version = hello["version"].as_str().unwrap_or_default();
    below(version, min_host).then(|| Mismatch::UpdateHost { version: version.to_owned(), minimum: min_host.to_owned() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn floors_never_exceed_this_release() {
        // A floor above the version that ships it would lock out even matching builds.
        let own = env!("CARGO_PKG_VERSION");
        assert!(!below(own, MIN_APP), "MIN_APP {MIN_APP} is above {own}");
        assert!(!below(own, MIN_HOST), "MIN_HOST {MIN_HOST} is above {own}");
        assert!(parse(MIN_APP).is_some() && parse(MIN_HOST).is_some());
    }

    #[test]
    fn parses_leniently() {
        assert_eq!(parse("2.4.1"), Some((2, 4, 1)));
        assert_eq!(parse(" v2.4 "), Some((2, 4, 0)));
        assert_eq!(parse("3"), Some((3, 0, 0)));
        assert_eq!(parse("2.4.0-beta.1+77"), Some((2, 4, 0)));
        assert_eq!(parse("2.10.0").cmp(&parse("2.9.9")), std::cmp::Ordering::Greater);
        for bad in ["", "abc", "2.x", "2.4.0.1", "-1.0.0", "2..0"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn unreadable_versions_never_block() {
        assert!(!below("", "2.4.0"));
        assert!(!below("2.0.0", "garbage"));
        assert!(below("2.3.9", "2.4.0"));
        assert!(!below("2.4.0", "2.4.0"));
        assert!(!below("2.4.0-dev", "2.4.0"));
    }

    #[test]
    fn host_updates_wait_for_the_iphone_app_only_when_needed() {
        let ok = |v: &str| -> anyhow::Result<Option<String>> { Ok(Some(v.to_owned())) };
        let wait = |store: Option<&str>| Gate::WaitForApp { required: "2.6.0".into(), store: store.map(Into::into) };
        // Same floor, or no iPhone here: the App Store doesn't matter.
        assert_eq!(gate("2.6.0", "2.6.0", true, &Err(anyhow::anyhow!("offline"))), Gate::Proceed);
        assert_eq!(gate("2.3.0", "2.6.0", false, &ok("2.2.1")), Gate::Proceed);
        // A raised floor waits for the App Store.
        assert_eq!(gate("2.3.0", "2.6.0", true, &ok("2.6.0")), Gate::Proceed);
        assert_eq!(gate("2.3.0", "2.6.0", true, &ok("2.7.1")), Gate::Proceed);
        assert_eq!(gate("2.3.0", "2.6.0", true, &ok("2.5.9")), wait(Some("2.5.9")));
        // No answer, or one that can't be read, isn't permission.
        assert_eq!(gate("2.3.0", "2.6.0", true, &Err(anyhow::anyhow!("offline"))), wait(None));
        assert_eq!(gate("2.3.0", "2.6.0", true, &Ok(None)), wait(None));
        assert_eq!(gate("2.3.0", "2.6.0", true, &ok("soon")), wait(Some("soon")));
    }

    #[test]
    fn decides_which_side_updates() {
        let host = |version: &str, min_app: Option<&str>| {
            let mut h = json!({"version": version});
            if let Some(m) = min_app {
                h["minApp"] = json!(m);
            }
            h
        };
        assert_eq!(check("2.4.0", "2.3.0", &host("2.4.0", Some("2.3.0"))), None);
        assert_eq!(
            check("2.4.0", "2.3.0", &host("2.6.0", Some("2.5.0"))),
            Some(Mismatch::UpdateApp { minimum: "2.5.0".into() })
        );
        assert_eq!(
            check("2.6.0", "2.5.0", &host("2.4.0", Some("2.3.0"))),
            Some(Mismatch::UpdateHost { version: "2.4.0".into(), minimum: "2.5.0".into() })
        );
        // Older hosts send no minApp; only the floor applies.
        assert_eq!(check("2.4.0", "2.3.0", &host("2.3.1", None)), None);
        assert_eq!(
            check("2.4.0", "2.3.0", &host("2.2.3", None)),
            Some(Mismatch::UpdateHost { version: "2.2.3".into(), minimum: "2.3.0".into() })
        );
        // A hello without a version can't be judged.
        assert_eq!(check("2.4.0", "2.3.0", &json!({})), None);
        // A development client with no readable version isn't locked out.
        assert_eq!(check("dev", "2.3.0", &host("2.6.0", Some("2.5.0"))), None);
    }
}
