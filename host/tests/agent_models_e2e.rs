//! `agentModels` routes each request through the selected `loginProfile`'s environment instead of
//! sharing or collapsing onto the machine's default login.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use serde_json::{Value, json};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn write_devin_shim(bin: &Path) {
    std::fs::create_dir_all(bin).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/models_agent.py");
    #[cfg(windows)]
    {
        let cmd = format!("@echo off\r\n{} -X utf8 -u \"{}\" %*\r\n", common::python(), fixture.display());
        std::fs::write(bin.join("devin.cmd"), cmd).unwrap();
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let script = format!("#!/bin/sh\nexec {} -X utf8 -u '{}' \"$@\"\n", common::python(), fixture.display());
        let file = bin.join("devin");
        std::fs::write(&file, script).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// A host whose PATH starts with the stand-in Devin under `shim_root`, the fixture's variables cleared, `extra` set.
/// Its model checks run for real against the stand-in, so it checks only Devin, in a home of its own: the computer's
/// other agents and the user's logins stay out.
async fn devin_host(shim_root: &Path, extra: &[(&str, &OsStr)]) -> Host {
    let bin = shim_root.join("bin");
    write_devin_shim(&bin);
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&inherited))).unwrap();
    let empty = OsStr::new("");
    let mut env = vec![
        ("PATH", path.as_os_str()),
        ("HOME", shim_root.as_os_str()),
        ("CODYNC_AGENT_CHECKS", OsStr::new("devin")),
        ("CODYNC_AGENT_ACCOUNT", empty),
        ("E2E_MODEL_TIER", empty),
        ("XDG_DATA_HOME", empty),
        ("XDG_STATE_HOME", empty),
    ];
    env.extend_from_slice(extra);
    Host::start_with(&env).await
}

/// The stand-in's flags for a probe on an account that waits: `(started, release)`, files under `shim_root`.
fn probe_flags(shim_root: &Path) -> (PathBuf, PathBuf) {
    (shim_root.join("probe.started"), shim_root.join("probe.release"))
}

async fn add_login_profile(host: &Host, kind: &str) -> String {
    host.call("addLoginProfile", json!({"backend": "devin", "kind": kind})).await["loginProfile"]["id"]
        .as_str()
        .unwrap()
        .to_owned()
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
    let host = devin_host(&shim_root, &[]).await;
    let account = add_login_profile(&host, "account").await;
    let api_key = add_login_profile(&host, "apiKey").await;
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

#[tokio::test]
async fn agent_models_holds_login_profile_against_removal_until_discovery_finishes() {
    if common::python_missing() {
        return;
    }
    let shim_root = std::env::temp_dir().join(format!("codync-agent-models-hold-{}", uuid::Uuid::new_v4()));
    let (started_flag, release_flag) = probe_flags(&shim_root);
    let host = Arc::new(
        devin_host(
            &shim_root,
            &[
                ("E2E_HOLD_ACCOUNT", OsStr::new("*")),
                ("E2E_PROBE_STARTED", started_flag.as_os_str()),
                ("E2E_PROBE_RELEASE", release_flag.as_os_str()),
            ],
        )
        .await,
    );
    let account = add_login_profile(&host, "account").await;
    let slot_dir = host.home.join("login-profiles").join("devin").join(&account);
    assert!(slot_dir.exists(), "account slot directory should exist before probe: {}", slot_dir.display());

    let probe_host = host.clone();
    let probe_account = account.clone();
    let probe_task = tokio::spawn(async move {
        probe_host.call("agentModels", json!({"backend": "devin", "loginProfile": probe_account})).await
    });

    common::eventually("agentModels probe starts on loginProfile", std::time::Duration::from_secs(15), || {
        let started = started_flag.clone();
        async move { started.exists() }
    })
    .await;

    // While discovery is in-flight, concurrent removal must be refused and the slot directory preserved.
    let err = host.refused("removeLoginProfile", json!({"loginProfile": account})).await;
    assert!(err.contains("still running") || err.contains("being checked"), "{err}");
    assert!(slot_dir.exists(), "account slot directory must not be deleted while probe runs");

    // Default login discovery stays independent and unblocked while the profile probe is paused.
    let default_catalog = host.call("agentModels", json!({"backend": "devin", "loginProfile": null})).await;
    assert_eq!(model_ids(&default_catalog), vec!["default-flash"]);

    // Missing profile is refused cleanly without spawning.
    let missing_err = host.refused("agentModels", json!({"backend": "devin", "loginProfile": "missing-id"})).await;
    assert!(missing_err.contains("removed"), "{missing_err}");

    // Unblock discovery; once `models()` calls `acp.kill().await` and drops `Running`, removal succeeds.
    std::fs::write(&release_flag, "go").unwrap();
    let acct_catalog = probe_task.await.unwrap();
    assert_eq!(model_ids(&acct_catalog), vec![format!("acct-{account}-pro"), "shared-flash".to_owned()]);

    host.call("removeLoginProfile", json!({"loginProfile": account})).await;
    assert!(!slot_dir.exists(), "account slot directory should be deleted once probe finishes");

    // Removing it leaves subsequent discovery on that profile cleanly refused.
    let gone_err = host.refused("agentModels", json!({"backend": "devin", "loginProfile": account})).await;
    assert!(gone_err.contains("removed"), "{gone_err}");

    let _ = std::fs::remove_dir_all(&shim_root);
}

#[tokio::test]
async fn agent_models_releases_login_profile_when_discovery_fails() {
    if common::python_missing() {
        return;
    }
    let shim_root = std::env::temp_dir().join(format!("codync-agent-models-fail-{}", uuid::Uuid::new_v4()));
    let (started_flag, release_flag) = probe_flags(&shim_root);
    let host = Arc::new(
        devin_host(
            &shim_root,
            &[
                ("E2E_HOLD_ACCOUNT", OsStr::new("*")),
                ("E2E_PROBE_STARTED", started_flag.as_os_str()),
                ("E2E_PROBE_RELEASE", release_flag.as_os_str()),
                ("E2E_PROBE_FAIL", OsStr::new("1")),
            ],
        )
        .await,
    );
    let account = add_login_profile(&host, "account").await;
    let slot_dir = host.home.join("login-profiles").join("devin").join(&account);
    assert!(slot_dir.exists());

    let probe_host = host.clone();
    let probe_account = account.clone();
    let probe_task = tokio::spawn(async move {
        probe_host.refused("agentModels", json!({"backend": "devin", "loginProfile": probe_account})).await
    });

    common::eventually("failing agentModels probe starts", std::time::Duration::from_secs(15), || {
        let started = started_flag.clone();
        async move { started.exists() }
    })
    .await;

    let err = host.refused("removeLoginProfile", json!({"loginProfile": account})).await;
    assert!(err.contains("still running") || err.contains("being checked"), "{err}");
    assert!(slot_dir.exists());

    std::fs::write(&release_flag, "go").unwrap();
    let probe_err = probe_task.await.unwrap();
    assert!(probe_err.contains("probe failed"), "{probe_err}");

    // Even after discovery errors out, `Running` was released after `acp.kill().await`.
    host.call("removeLoginProfile", json!({"loginProfile": account})).await;
    assert!(!slot_dir.exists());

    let _ = std::fs::remove_dir_all(&shim_root);
}
