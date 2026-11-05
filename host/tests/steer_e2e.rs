//! A real host with the user's messages queued behind a running turn: steered into it, interrupting a turn whose agent
//! can't steer, first in line behind other work, left for the next turn when the agent says so or refuses, or taken
//! back. A turn that ends before the agent answers: `steer_turn_end_e2e`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::host::Host;
use common::steer::{entries, entry, gone, idle, queue_behind_a_turn, status, turn_running};
use common::{eventually, python_missing};
use serde_json::{Value, json};
use std::time::Duration;

#[tokio::test]
async fn a_steered_message_joins_the_running_turn() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Steers", "steer_agent.py", "steer 30 1").await;
    let (first, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    // Pressed twice while the agent decides: it goes to the agent once.
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the turn answered both", Duration::from_secs(20), async || {
        status(&host, &bot, &second).await == "sent" && idle(&host, &bot).await
    })
    .await;

    let steered = host.requests(&bot, "_session/steering");
    assert_eq!(steered.len(), 1);
    assert!(steered[0]["params"]["prompt"][0]["text"].as_str().unwrap().contains("second"));
    assert_eq!(steered[0]["params"]["_meta"]["steering"]["idleBehavior"], "promptRequired");
    assert_eq!(host.requests(&bot, "session/prompt").len(), 1, "no turn of its own");
    let (first, second) = (entry(&host, &bot, &first).await, entry(&host, &bot, &second).await);
    assert_eq!(second["turn"], first["turn"], "it is the running turn's");
    let reply = entries(&host, &bot).await.into_iter().rev().find(|e| e["data"]["final"] == true).unwrap();
    assert!(reply["data"]["text"].as_str().unwrap().contains("Got: "), "{reply}");
    assert!(reply["seq"].as_i64() > second["seq"].as_i64(), "the reply comes after the message it answers");
}

#[tokio::test]
async fn a_steered_message_gets_its_reply_even_after_the_bot_used_send_message() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Sends", "steer_agent.py", "sends 30").await;
    let (_, second) = queue_behind_a_turn(&host, &bot).await;
    let is_final = async |text: &str| {
        entries(&host, &bot)
            .await
            .iter()
            .find(|e| e["data"]["text"].as_str().is_some_and(|t| t.starts_with(text)))
            .map(|e| e["data"]["final"] == true)
    };
    eventually("the bot told the user something", Duration::from_secs(20), async || {
        is_final("Before it joined").await == Some(true)
    })
    .await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the turn ended", Duration::from_secs(20), async || {
        status(&host, &bot, &second).await == "sent" && idle(&host, &bot).await
    })
    .await;

    assert_eq!(is_final("After it joined").await, Some(true));
    assert_eq!(is_final("Working").await, Some(false), "what it wrote before a send stays in the trace");
    assert_eq!(is_final("About to send").await, Some(false), "so does what it wrote before a later send");
    let reply = entries(&host, &bot).await.into_iter().rev().find(|e| e["data"]["final"] == true).unwrap();
    assert!(reply["data"]["text"].as_str().unwrap().contains("Got: "), "{reply}");
    assert!(reply["seq"].as_i64() > entry(&host, &bot, &second).await["seq"].as_i64());
}

#[tokio::test]
async fn behind_other_work_a_steered_message_goes_first_in_line() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Behind", "steer_agent.py", "steer 5").await;
    let mut routines = vec![];
    for name in ["The running routine", "The waiting routine"] {
        let body = json!({"botId": bot, "name": name, "instruction": name, "triggers": [{"type": "webhook"}], "timeoutSeconds": 60});
        routines.push(host.call("saveRoutine", body).await["routine"]["id"].as_str().unwrap().to_owned());
    }
    host.call("runRoutine", json!({"botId": bot, "id": routines[0]})).await;
    eventually("the routine's turn runs", Duration::from_mins(1), async || {
        host.requests(&bot, "session/prompt").len() == 1
    })
    .await;
    host.call("runRoutine", json!({"botId": bot, "id": routines[1]})).await;
    let second = host.call("send", json!({"botId": bot, "text": "second"})).await["entry"].clone();
    assert_eq!(second["data"]["status"], "queued");
    host.call("steer", json!({"botId": bot, "entryId": second["id"]})).await;
    eventually("all three had their turns", Duration::from_secs(30), async || {
        host.requests(&bot, "session/prompt").len() == 3
    })
    .await;

    let prompts: Vec<String> =
        host.requests(&bot, "session/prompt").iter().map(|r| r["params"]["prompt"][0]["text"].to_string()).collect();
    for (prompt, said) in prompts.iter().zip(["The running routine", "second", "The waiting routine"]) {
        assert!(prompt.contains(said), "{said}: {prompts:?}");
    }
    assert_eq!(host.requests(&bot, "_session/steering"), Vec::<Value>::new(), "not the user's turn to join");
    assert_eq!(host.requests(&bot, "session/cancel"), Vec::<Value>::new(), "not the user's turn to stop");
}

#[tokio::test]
async fn steering_an_agent_that_cannot_stops_the_turn_for_the_message() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Plain", "steer_agent.py", "plain 30").await;
    let (_, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the message started its own turn", Duration::from_secs(20), async || {
        host.requests(&bot, "session/prompt").len() == 2
    })
    .await;
    assert_eq!(host.requests(&bot, "_session/steering"), Vec::<Value>::new());
    assert_eq!(host.requests(&bot, "session/cancel").len(), 1);
    let prompt = &host.requests(&bot, "session/prompt")[1]["params"]["prompt"][0]["text"];
    assert!(prompt.as_str().unwrap().contains("second"), "{prompt}");
    assert_eq!(status(&host, &bot, &second).await, "sent");
    host.call("stop", json!({"botId": bot})).await;
}

#[tokio::test]
async fn a_message_the_agent_leaves_for_later_is_the_next_turn() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Later", "steer_agent.py", "next 5").await;
    let (first, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the message started its own turn", Duration::from_secs(20), async || {
        host.requests(&bot, "session/prompt").len() == 2
    })
    .await;
    assert_eq!(host.requests(&bot, "_session/steering").len(), 1);
    assert_eq!(host.requests(&bot, "session/cancel"), Vec::<Value>::new(), "the first turn ran to its end");
    let prompt = &host.requests(&bot, "session/prompt")[1]["params"]["prompt"][0]["text"];
    assert!(prompt.as_str().unwrap().contains("second"), "{prompt}");
    let (first, second) = (entry(&host, &bot, &first).await, entry(&host, &bot, &second).await);
    assert_eq!(second["data"]["status"], "sent");
    assert_ne!(second["turn"], first["turn"], "a turn of its own");
    host.call("stop", json!({"botId": bot})).await;
}

#[tokio::test]
async fn a_queued_message_taken_back_never_goes_out() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Takes back", "steer_agent.py", "plain 5").await;
    let (first, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("unqueue", json!({"botId": bot, "entryId": second})).await;
    eventually("the roster stopped previewing it", Duration::from_secs(20), async || {
        !common::bot(&host.snapshot().await, &bot)["lastMessage"].as_str().unwrap().contains("second")
    })
    .await;
    eventually("the first turn ended", Duration::from_secs(20), async || idle(&host, &bot).await).await;
    tokio::time::sleep(Duration::from_millis(500)).await;

    assert_eq!(host.requests(&bot, "session/prompt").len(), 1, "the second never went out");
    assert!(gone(&host, &bot, &second).await, "history no longer lists it");
    // Only a message still waiting can be steered or taken back.
    for method in ["steer", "unqueue"] {
        for id in [&first, &second] {
            let error = host.refused(method, json!({"botId": bot, "entryId": id})).await;
            assert!(error.contains("isn't waiting"), "{method} {id}: {error}");
        }
    }
}

#[tokio::test]
async fn a_message_taken_back_while_the_agent_decides_goes_unless_the_agent_took_it() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    // The agent answers each steer a second later: it leaves one for later, and takes the other.
    let (left, taken) = (
        host.fixture_bot("Leaves it", "steer_agent.py", "next 30 1").await,
        host.fixture_bot("Takes it", "steer_agent.py", "steer 30 1").await,
    );
    let (_, left_alone) = queue_behind_a_turn(&host, &left).await;
    let (_, joined) = queue_behind_a_turn(&host, &taken).await;
    for (bot, id) in [(&left, &left_alone), (&taken, &joined)] {
        host.call("steer", json!({"botId": bot, "entryId": id})).await;
        host.call("unqueue", json!({"botId": bot, "entryId": id})).await;
    }
    eventually("the agent left it, and it went", Duration::from_secs(20), async || {
        gone(&host, &left, &left_alone).await
    })
    .await;
    eventually("the agent took the other", Duration::from_secs(20), async || {
        status(&host, &taken, &joined).await == "sent"
    })
    .await;
    assert_eq!(entry(&host, &taken, &joined).await["data"]["removed"], Value::Null, "the chat shows it again");
    host.call("stop", json!({"botId": left})).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(host.requests(&left, "session/prompt").len(), 1, "it never went out");
}

#[tokio::test]
async fn a_queued_message_taken_back_takes_its_files_with_it() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Files", "steer_agent.py", "plain 5").await;
    turn_running(&host, &bot).await;
    let upload = uuid::Uuid::new_v4().to_string();
    let chunk =
        json!({"botId": bot, "uploadId": upload, "name": "secret.pem", "offset": 0, "data": "aGk=", "done": true});
    host.call("upload", chunk).await;
    let sent = host.call("send", json!({"botId": bot, "text": "", "attachments": [upload]})).await;
    let read = json!({"botId": bot, "uploadId": upload, "offset": 0});
    assert!(host.post("readUpload", &read).await.0, "the file is there while it waits");
    host.call("unqueue", json!({"botId": bot, "entryId": sent["entry"]["id"]})).await;
    let folder = host.home.join("bots").join(&bot).join("workspace/uploads").join(&upload);
    eventually("its file left the bot's folder", Duration::from_secs(10), async || !folder.exists()).await;
    assert!(!host.post("readUpload", &read).await.0, "nor can a client read it");
    host.call("stop", json!({"botId": bot})).await;
}

#[tokio::test]
async fn a_message_the_agent_leaves_for_later_goes_before_other_work() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Ahead", "steer_agent.py", "next 3").await;
    turn_running(&host, &bot).await;
    let body = json!({"botId": bot, "name": "Other work", "instruction": "Other work", "triggers": [{"type": "webhook"}], "timeoutSeconds": 60});
    let routine = host.call("saveRoutine", body).await["routine"]["id"].clone();
    host.call("runRoutine", json!({"botId": bot, "id": routine})).await;
    let second = host.call("send", json!({"botId": bot, "text": "second"})).await["entry"]["id"].clone();
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("all three had their turns", Duration::from_secs(30), async || {
        host.requests(&bot, "session/prompt").len() == 3
    })
    .await;
    let prompts: Vec<String> =
        host.requests(&bot, "session/prompt").iter().map(|r| r["params"]["prompt"][0]["text"].to_string()).collect();
    for (prompt, said) in prompts.iter().zip(["first", "second", "Other work"]) {
        assert!(prompt.contains(said), "{said}: {prompts:?}");
    }
}

#[tokio::test]
async fn a_message_moved_first_takes_the_earlier_turn() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Reorders", "steer_agent.py", "plain 3").await;
    let (_, second) = queue_behind_a_turn(&host, &bot).await;
    let third = host.call("send", json!({"botId": bot, "text": "third"})).await["entry"]["id"].clone();
    host.call("steer", json!({"botId": bot, "entryId": third})).await;
    eventually("both started one turn, and it ended", Duration::from_secs(20), async || {
        host.requests(&bot, "session/prompt").len() == 2 && idle(&host, &bot).await
    })
    .await;
    let prompt = host.requests(&bot, "session/prompt")[1]["params"]["prompt"][0]["text"].to_string();
    assert!(prompt.find("third") < prompt.find("second"), "{prompt}");
    let (second, third) = (entry(&host, &bot, &second).await, entry(&host, &bot, third.as_str().unwrap()).await);
    assert!(third["turn"].as_i64() < second["turn"].as_i64(), "turns follow the order they went out in");
    let reply = entries(&host, &bot).await.into_iter().rev().find(|e| e["data"]["final"] == true).unwrap();
    assert_eq!(reply["turn"], second["turn"], "the agent works in the batch's last turn, the highest");
}

#[tokio::test]
async fn a_message_steered_after_stop_waits_for_the_stopping_turn_to_end() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Lingers", "steer_agent.py", "lingers 30").await;
    turn_running(&host, &bot).await;
    host.call("stop", json!({"botId": bot})).await;
    let second = host.call("send", json!({"botId": bot, "text": "second"})).await["entry"].clone();
    assert_eq!(second["data"]["status"], "queued", "the stopped turn is still winding down");
    host.call("steer", json!({"botId": bot, "entryId": second["id"]})).await;
    eventually("the message started its own turn", Duration::from_secs(20), async || {
        host.requests(&bot, "session/prompt").len() == 2
    })
    .await;
    assert_eq!(host.requests(&bot, "_session/steering"), Vec::<Value>::new(), "a stopping turn takes nothing in");
    let prompt = &host.requests(&bot, "session/prompt")[1]["params"]["prompt"][0]["text"];
    assert!(prompt.as_str().unwrap().contains("second"), "{prompt}");
    host.call("stop", json!({"botId": bot})).await;
}

#[tokio::test]
async fn a_message_the_agent_refuses_waits_for_the_next_turn() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Refuses", "steer_agent.py", "refuse 3").await;
    let (_, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    eventually("the agent got the steer", Duration::from_secs(20), async || {
        host.requests(&bot, "_session/steering").len() == 1
    })
    .await;
    assert_eq!(status(&host, &bot, &second).await, "queued");
    eventually("the message started its own turn", Duration::from_secs(20), async || {
        host.requests(&bot, "session/prompt").len() == 2
    })
    .await;
    let prompt = &host.requests(&bot, "session/prompt")[1]["params"]["prompt"][0]["text"];
    assert!(prompt.as_str().unwrap().contains("second"), "{prompt}");
    assert_eq!(status(&host, &bot, &second).await, "sent");
    host.call("stop", json!({"botId": bot})).await;
}

#[tokio::test]
async fn a_stop_while_the_agent_decides_keeps_the_message_dropped() {
    if python_missing() {
        return;
    }
    let host = Host::start().await;
    let bot = host.fixture_bot("Stops", "steer_agent.py", "steer 30 1").await;
    let (_, second) = queue_behind_a_turn(&host, &bot).await;
    host.call("steer", json!({"botId": bot, "entryId": second})).await;
    host.call("stop", json!({"botId": bot})).await;
    eventually("the agent answered", Duration::from_secs(10), async || idle(&host, &bot).await).await;
    tokio::time::sleep(Duration::from_secs(2)).await;
    let dropped = entry(&host, &bot, &second).await;
    assert_eq!(dropped["data"]["status"], "cancelled", "the agent's late yes doesn't bring it back");
    assert_eq!(dropped["data"]["removed"], Value::Null, "the chat says it wasn't sent");
    assert_eq!(host.requests(&bot, "session/prompt").len(), 1);
}
