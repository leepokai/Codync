//! What a bot leaves running through its shell.

use super::*;

#[tokio::test]
async fn what_an_agent_leaves_running_shows_and_can_be_stopped() {
    let host = Host::start().await;
    let port = free_low_port();
    let bot = agent_bot(&host, port).await;
    host.reply(&bot, "run it").await;
    let list = wait_for(&host, &bot, "the agent's server shows", |list| {
        list.iter().any(|s| s["managed"] == false && s["ports"][0]["port"] == port)
    })
    .await;
    let found = list.iter().find(|s| s["managed"] == false).unwrap();
    assert_eq!(found["status"], "running");
    assert!(found["id"].as_str().unwrap().starts_with("detected:"));

    // It can't start again, so asking leaves it running.
    let refused = host.refused("devServerRestart", json!({"botId": bot, "id": found["id"]})).await;
    assert!(refused.contains("unknown dev server"), "{refused}");
    assert!(serves(port));

    stop(&host, &bot, &found["id"], port).await;
    wait_for(&host, &bot, "it no longer shows", <[Value]>::is_empty).await;
    // An id that was never found stops nothing.
    let error = host.refused("devServerStop", json!({"botId": bot, "id": "detected:1"})).await;
    assert!(error.contains("unknown dev server"), "{error}");
}

/// A `ps` that fails once a file named `broken` sits beside it, in a folder of its own to put first on a host's PATH.
#[cfg(unix)]
fn breakable_ps() -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let folder = std::env::temp_dir().join(format!("codync-ps-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).unwrap();
    let script = format!("#!/bin/sh\n[ -e '{}/broken' ] && exit 1\nexec /bin/ps \"$@\"\n", folder.display());
    std::fs::write(folder.join("ps"), script).unwrap();
    std::fs::set_permissions(folder.join("ps"), std::fs::Permissions::from_mode(0o755)).unwrap();
    folder
}

#[cfg(unix)]
#[tokio::test]
async fn stopping_what_an_agent_left_running_fails_while_its_processes_cant_be_read() {
    let folder = breakable_ps();
    let path = std::env::join_paths(
        std::iter::once(folder.clone()).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let host = Host::start_with(&[("PATH", path.as_os_str())]).await;
    let port = free_low_port();
    let bot = agent_bot(&host, port).await;
    host.reply(&bot, "run it").await;
    let list = wait_for(&host, &bot, "the agent's server shows", |list| {
        list.iter().any(|s| s["managed"] == false && s["ports"][0]["port"] == port)
    })
    .await;
    let id = list.iter().find(|s| s["managed"] == false).unwrap()["id"].clone();

    std::fs::write(folder.join("broken"), "").unwrap();
    let refused = host.refused("devServerStop", json!({"botId": bot, "id": id})).await;
    assert!(refused.contains("couldn't check the agent's processes"), "{refused}");
    assert!(serves(port));
    assert!(servers(&host, &bot).await.iter().any(|s| s["id"] == id), "it still shows");

    std::fs::remove_file(folder.join("broken")).unwrap();
    stop(&host, &bot, &id, port).await;
    std::fs::remove_dir_all(&folder).unwrap();
}
