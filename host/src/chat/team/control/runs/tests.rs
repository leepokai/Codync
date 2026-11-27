#![allow(clippy::unwrap_used)]
use super::super::access::{Code, Level, Refusal, Scope};
use super::super::clients::Principal;
use super::super::{call, clients};
use crate::chat::calls::context::tests::{bot, hub};
use serde_json::{Value, json};

fn app(level: Level, bots: Option<Vec<String>>) -> Principal {
    Principal {
        id: "a".into(),
        label: "App a".into(),
        approved: level,
        scopes: vec![Scope::Routines, Scope::Workflows],
        bots,
        cloud: false,
        folders: Vec::new(),
        plain_threads: false,
        via: None,
        session: None,
    }
}

fn code(e: &anyhow::Error) -> Code {
    e.downcast_ref::<Refusal>().unwrap_or_else(|| panic!("not a refusal: {e:#}")).code
}

#[tokio::test]
async fn an_app_lists_and_runs_routines_only_of_the_bots_it_may_use_and_a_retry_runs_once() {
    let hub = hub();
    clients::set_levels(&hub.store, &json!({"max": "full"})).unwrap();
    for id in ["b1", "b2"] {
        hub.store.save_bot(&bot(id, "Mika", "/tmp")).unwrap();
    }
    let routine = json!({"name": "Check", "instruction": "x", "triggers": [{"type": "interval", "seconds": 1800}]});
    let mut ids = Vec::new();
    for id in ["b1", "b2"] {
        ids.push(hub.routines.save(&hub, id, &routine).unwrap()["routine"]["id"].as_str().unwrap().to_owned());
    }
    let a = app(Level::Full, Some(vec!["b1".into()]));
    let listed = call(&hub, &a, "routine_list", &json!({})).await.unwrap();
    let bots: Vec<&Value> = listed["bots"].as_array().unwrap().iter().map(|b| &b["botId"]).collect();
    assert_eq!(bots, [&json!("b1")], "only the bots it may use");
    let other = call(&hub, &a, "routine_list", &json!({"botId": "b2"})).await.unwrap_err();
    assert_eq!(code(&other), Code::NotFound);
    let unknown = json!({"botId": "b2", "routineId": ids[1]});
    assert_eq!(code(&call(&hub, &a, "routine_run", &unknown).await.unwrap_err()), Code::NotFound);
    assert_eq!(code(&call(&hub, &a, "routine_run", &json!({"botId": "b1"})).await.unwrap_err()), Code::InvalidRequest);
    let run = json!({"botId": "b1", "routineId": ids[0], "clientRequestId": "r1"});
    let first = call(&hub, &a, "routine_run", &run).await.unwrap();
    let again = call(&hub, &a, "routine_run", &run).await.unwrap();
    assert_eq!((&first["replayed"], &again["replayed"]), (&json!(false), &json!(true)));
    assert_eq!(first["run"]["run"]["id"], again["run"]["run"]["id"], "the same run");
    assert_eq!(hub.routines.list("b1")["runs"].as_array().unwrap().len(), 1, "queued once");
    // Below Full it may list them, never run them.
    let auto = app(Level::Auto, None);
    assert!(call(&hub, &auto, "routine_list", &json!({})).await.is_ok());
    assert_eq!(code(&call(&hub, &auto, "routine_run", &run).await.unwrap_err()), Code::CapabilityDenied);
}

#[tokio::test]
async fn workflows_are_not_limited_by_bot_and_a_run_names_its_workflow() {
    let hub = hub();
    clients::set_levels(&hub.store, &json!({"max": "full"})).unwrap();
    hub.workflows.create(&hub, "Ship", None, None, None, None).unwrap();
    // An app given no bot at all sees every workflow, as the user does (docs/features/mcp-access.md).
    let a = app(Level::Full, Some(Vec::new()));
    let listed = call(&hub, &a, "workflow_list", &json!({})).await.unwrap();
    assert_eq!(listed, hub.workflows.list(&hub).unwrap());
    assert_eq!(listed["workflows"].as_array().unwrap().len(), 1);
    assert_eq!(code(&call(&hub, &a, "workflow_run", &json!({})).await.unwrap_err()), Code::InvalidRequest);
}
