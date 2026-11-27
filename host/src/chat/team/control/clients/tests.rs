#![allow(clippy::unwrap_used)]
use super::*;
use crate::chat::calls::context::tests::{bot, hub};
use crate::chat::team::control::calls::record;

fn grant(v: Value) -> Grant {
    serde_json::from_value(v).unwrap()
}

#[test]
fn a_token_is_shown_once_kept_only_as_a_hash_and_works_on_its_own() {
    let hub = hub();
    let (client, token) = create(&hub, grant(json!({"label": "Laptop agent", "access": "auto"}))).unwrap();
    assert!(token.starts_with(TOKEN_PREFIX) && token.len() > 64);
    let kept = hub.store.kv_get(KV).unwrap();
    assert!(!kept.contains(&token), "the raw token is never kept");
    assert!(client.get("tokenHash").is_none() && !list(&hub.store).to_string().contains("tokenHash"));
    let p = local(&hub.store, Some(&token)).unwrap();
    assert_eq!(
        (p.label.as_str(), p.approved, p.scopes.as_slice()),
        ("Laptop agent", Level::Auto, &[Scope::Threads][..])
    );
    assert!(load(&hub.store)[0].last_used_at.is_some(), "its use is noted");
    assert_eq!(
        (&client["bots"], &client["plainThreads"], &client["scopes"]),
        (&json!(["*"]), &json!(true), &json!(["threads"])),
        "what every client gives a new app"
    );
    assert_eq!(command("cdy_mcp_x"), "CODYNC_MCP_TOKEN=cdy_mcp_x codync-host mcp network");
    let wrong = local(&hub.store, Some("cdy_mcp_nope")).unwrap_err();
    assert_eq!(wrong.downcast_ref::<Refusal>().unwrap().code, Code::Revoked);
}

#[test]
fn a_revoked_or_expired_app_is_refused() {
    let hub = hub();
    let (client, token) = create(&hub, grant(json!({"label": "A"}))).unwrap();
    let id = client["id"].as_str().unwrap();
    update(&hub.store, id, grant(json!({"days": 1}))).unwrap();
    let renewed = update(&hub.store, id, grant(json!({"renew": true}))).unwrap();
    assert!(renewed["expiresAt"].as_i64().unwrap() > now_ms() + (DEFAULT_DAYS - 1) * DAY_MS, "renewed for 30 days");
    change(&hub.store, |all| {
        all[0].expires_at = now_ms() - 1;
        Ok(())
    })
    .unwrap();
    let expired = local(&hub.store, Some(&token)).unwrap_err();
    assert_eq!(expired.downcast_ref::<Refusal>().unwrap().code, Code::Expired);
    revoke(&hub, id).unwrap();
    let revoked = local(&hub.store, Some(&token)).unwrap_err();
    assert_eq!(revoked.downcast_ref::<Refusal>().unwrap().code, Code::Revoked);
    assert!(update(&hub.store, id, grant(json!({"access": "full"}))).is_err(), "a revoked app isn't widened again");
}

#[test]
fn a_grant_is_checked() {
    let hub = hub();
    hub.store.save_bot(&bot("b1", "Mika", "/tmp")).unwrap();
    assert!(create(&hub, grant(json!({}))).is_err(), "it needs a name");
    assert!(create(&hub, grant(json!({"label": "x", "days": 91}))).is_err(), "at most 90 days");
    assert!(create(&hub, grant(json!({"label": "x", "bots": ["nobody"]}))).is_err());
    assert!(create(&hub, grant(json!({"label": "x", "bots": ["b1", "*"]}))).is_ok());
    assert!(create(&hub, grant(json!({"label": "x", "folders": ["relative/path"]}))).is_err());
}

#[test]
fn a_folder_an_app_may_use_is_real_and_never_codyncs_own() {
    let dir = std::env::temp_dir().join(format!("codync-mcp-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("inside")).unwrap();
    let real = allowed_folder(dir.join("inside").to_str().unwrap()).unwrap();
    assert!(Path::new(&real).is_absolute() && real.ends_with("inside"));
    assert!(allowed_folder("inside").is_err(), "a full path");
    assert!(allowed_folder(dir.join("missing").to_str().unwrap()).is_err());
    let data = crate::service::data_dir();
    std::fs::create_dir_all(&data).unwrap();
    assert!(allowed_folder(data.to_str().unwrap()).is_err(), "Codync's data");
    #[cfg(unix)]
    {
        let link = dir.join("link");
        std::os::unix::fs::symlink(&data, &link).unwrap();
        assert!(allowed_folder(link.to_str().unwrap()).is_err(), "a link into Codync's data is followed");
    }
}

#[test]
fn the_computers_levels_default_to_the_least_and_the_audit_keeps_no_arguments() {
    let hub = hub();
    assert_eq!(
        (max_access(&hub.store), local_access(&hub.store), cloud_access(&hub.store)),
        (Level::Auto, Level::Ask, Level::ReadOnly)
    );
    set_levels(&hub.store, &json!({"cloud": "ask", "max": "full"})).unwrap();
    assert_eq!((max_access(&hub.store), cloud(&hub.store).approved), (Level::Full, Level::Ask));
    assert!(set_levels(&hub.store, &json!({"local": "root"})).is_err());
    record(&hub.store, &json!({"client": LOCAL, "tool": "thread_list", "outcome": "ok", "at": 1}));
    assert_eq!(list(&hub.store)["recent"][LOCAL][0]["tool"], "thread_list");
}
