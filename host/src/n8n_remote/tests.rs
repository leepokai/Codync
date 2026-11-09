use super::*;
use crate::api::devices::Caller;

#[test]
fn a_phone_never_gets_the_editors_session_cookie_but_this_computer_does() {
    let state = json!({"state": "running", "port": 5678, "auth": "automan-auth=secret.jwt"});
    assert_eq!(view(&Caller::Local, state.clone())["auth"], "automan-auth=secret.jwt");
    let phone = view(&Caller::Device { key: "phone".into(), scopes: vec![] }, state.clone());
    assert_eq!(view(&Caller::Pairing { key: "p".into() }, state)["auth"], Value::Null);
    assert_eq!(phone["auth"], Value::Null, "the key stays, with no value");
    assert_eq!(phone["state"], "running", "everything else is untouched");
    assert!(!phone.to_string().contains("secret"));
}

#[test]
fn only_ids_n8n_would_make_reach_a_url() {
    for good in ["12", "VmwOO9HeTEj20kxM", "a-b_c"] {
        assert!(valid_id(good), "{good}");
    }
    for bad in ["", "../rest/login", "a/b", "a b", "x?y=1", "%2e%2e", &"a".repeat(65)] {
        assert!(!valid_id(bad), "{bad}");
    }
}

#[test]
fn the_key_n8n_shows_once_is_found_in_either_shape_and_a_masked_one_is_refused() {
    assert_eq!(raw_key(&json!({"rawApiKey": "abc"})), Some("abc"));
    assert_eq!(raw_key(&json!({"data": {"rawApiKey": "abc"}})), Some("abc"));
    assert_eq!(raw_key(&json!({"apiKey": "******xyz"})), None, "only the raw key is usable");
    assert_eq!(raw_key(&json!({"rawApiKey": "******xyz"})), None);
    assert_eq!(raw_key(&json!({})), None);
}

#[test]
fn workflows_say_whether_they_are_published_by_either_field_and_skip_archived_ones() {
    let body = json!({
        "data": [
            {"id": "1", "name": "Daily report", "activeVersionId": "v9", "updatedAt": "2026-11-01T10:00:00.000Z"},
            {"id": "2", "name": "Old style", "active": true},
            {"id": "3", "name": "Draft", "activeVersionId": null, "active": false},
            {"id": "4", "name": "Gone", "activeVersionId": "v1", "isArchived": true},
            {"name": "No id"},
        ],
        "nextCursor": "abc",
    });
    let list = workflow_list(&body);
    let names: Vec<&str> =
        list["workflows"].as_array().expect("a list").iter().filter_map(|w| w["name"].as_str()).collect();
    assert_eq!(names, ["Daily report", "Old style", "Draft"]);
    assert_eq!(list["workflows"][0]["active"], true);
    assert_eq!(list["workflows"][1]["active"], true);
    assert_eq!(list["workflows"][2]["active"], false);
    assert_eq!(list["workflows"][0]["updatedAt"], "2026-11-01T10:00:00.000Z");
    assert_eq!(list["more"], true, "there are more than the page");
    assert_eq!(workflow_list(&json!({"data": []}))["more"], false);
    assert_eq!(workflow_list(&Value::Null)["workflows"], json!([]), "an odd reply is an empty list, not a crash");
}

#[test]
fn executions_keep_what_a_card_needs_and_read_numeric_ids() {
    let body = json!({"data": [
        {"id": "1000", "workflowId": "7", "status": "success", "mode": "trigger", "startedAt": "2026-11-01T10:00:00.000Z", "stoppedAt": "2026-11-01T10:00:02.000Z", "finished": true},
        {"id": 1001, "workflowId": 7, "status": "running", "mode": "manual", "startedAt": null, "stoppedAt": null},
        {"workflowId": "7"},
    ]});
    let list = execution_list(&body);
    let rows = list["executions"].as_array().expect("a list");
    assert_eq!(rows.len(), 2, "a run with no id is dropped");
    assert_eq!(rows[0]["status"], "success");
    assert_eq!(rows[0]["stoppedAt"], "2026-11-01T10:00:02.000Z");
    assert_eq!(rows[1]["id"], "1001");
    assert_eq!(rows[1]["workflowId"], "7");
    assert_eq!(rows[1]["startedAt"], Value::Null);
    assert!(rows[0].get("finished").is_none(), "n8n's deprecated field stays out");
}
