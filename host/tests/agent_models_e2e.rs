//! `agentModels` routes each request through the selected `loginProfile`'s environment instead of
//! sharing or collapsing onto the machine's default login.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use serde_json::{Value, json};
use std::path::Path;

fn write_devin_shim(bin: &Path) {
    std::fs::create_dir_all(bin).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models_agent.py");
    if cfg!(windows) {
        let cmd = format!("@echo off\r\n{} -X utf8 -u \"{}\" %*\r\n", common::python(), fixture.display());
        std::fs::write(bin.join("devin.cmd"), cmd).unwrap();
    } else {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let script = format!("#!/bin/sh\nexec {} -X utf8 -u '{}' \"$@\"\n", common::python(), fixture.display());
            let file = bin.join("devin");
            std::fs::write(&file, script).unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
}

fn model_ids(catalog: &Value) -> Vec<String> {
    catalog["models"].as_array().unwrap().iter().filter_map(|m| m["id"].as_str().map(str::to_owned)).collect()
}

#[tokio::test]
async fn agent_models_scopes_discovery_by_login_profile() {
    if common::python_missing() {
        return;
    }
    let shim_root = std::env::temp_dir().join(format!("codync-agent-models-shim-{}", uuid::Uuid::new_v4()));
    let bin = shim_root.join("bin");
    write_devin_shim(&bin);

    let inherited_path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&inherited_path));
    let path = std::env::join_paths(paths).unwrap();

    let empty = std::ffi::OsStr::new("");
    let host = common::host::Host::start_with(&[
        ("PATH", path.as_os_str()),
        ("CODYNC_AGENT_ACCOUNT", empty),
        ("E2E_MODEL_TIER", empty),
        ("XDG_DATA_HOME", empty),
        ("XDG_STATE_HOME", empty),
    ])
    .await;

    let account =
        host.call("addLoginProfile", json!({"backend": "devin", "kind": "account"})).await["loginProfile"]["id"]
            .as_str()
            .unwrap()
            .to_owned();

    let api_key =
        host.call("addLoginProfile", json!({"backend": "devin", "kind": "apiKey"})).await["loginProfile"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
    host.call("setLoginProfileKeys", json!({"loginProfile": api_key, "vars": {"E2E_MODEL_TIER": "ultra"}})).await;

    // Machine default (omitted, null, and empty string all map to default).
    let omitted = host.call("agentModels", json!({"backend": "devin"})).await;
    assert_eq!(model_ids(&omitted), vec!["default-flash"]);

    let null_profile = host.call("agentModels", json!({"backend": "devin", "loginProfile": null})).await;
    assert_eq!(model_ids(&null_profile), vec!["default-flash"]);

    let empty_profile = host.call("agentModels", json!({"backend": "devin", "loginProfile": ""})).await;
    assert_eq!(model_ids(&empty_profile), vec!["default-flash"]);

    // Account login profile receives CODYNC_AGENT_ACCOUNT and its own slot env.
    let acct_catalog = host.call("agentModels", json!({"backend": "devin", "loginProfile": account})).await;
    assert_eq!(model_ids(&acct_catalog), vec![format!("acct-{account}-pro"), "shared-flash".to_owned()]);

    // API key login profile receives its vaulted env vars.
    let ultra_catalog = host.call("agentModels", json!({"backend": "devin", "loginProfile": api_key})).await;
    assert_eq!(model_ids(&ultra_catalog), vec!["ultra-neon", "ultra-carbon"]);

    // Non-string loginProfile is rejected cleanly.
    let err = host.refused("agentModels", json!({"backend": "devin", "loginProfile": 123})).await;
    assert!(err.contains("loginProfile"), "{err}");

    let _ = std::fs::remove_dir_all(&shim_root);
}
