//! Follow-ups a bot sets for itself, and the progress note a routine's runs hand on.

use super::*;

/// Makes every waiting follow-up due while the host is down, then starts it again: a computer that was off when
/// one came due.
async fn restart_with_follow_ups_due(host: &mut Host) {
    common::stop(&mut host.child);
    {
        let db = rusqlite::Connection::open(host.home.join("codync.db")).unwrap();
        let raw: String = db.query_row("SELECT v FROM kv WHERE k = 'routines.v1'", [], |r| r.get(0)).unwrap();
        let mut state: Value = serde_json::from_str(&raw).unwrap();
        for follow_up in state["follow_ups"].as_array_mut().unwrap() {
            follow_up["at"] = 0.into();
        }
        db.execute("UPDATE kv SET v = ?1 WHERE k = 'routines.v1'", [state.to_string()]).unwrap();
    }
    (host.child, host.base) = common::serve(&host.home).await;
}

async fn follow_up(host: &Host, bot: &str, note: &str) -> Value {
    host.call("routineCall", json!({"botId":bot,"name":"follow_up","arguments":{"minutes":1,"note":note}})).await
}

async fn entries(host: &Host, bot: &str) -> Vec<Value> {
    host.call("history", json!({"botId":bot})).await["entries"].as_array().unwrap().clone()
}

fn finals(entries: &[Value]) -> usize {
    entries.iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true).count()
}

fn notices(entries: &[Value]) -> Vec<&str> {
    entries.iter().filter(|e| e["kind"] == "notice").filter_map(|e| e["data"]["text"].as_str()).collect()
}

#[tokio::test]
async fn a_follow_up_takes_one_turn_in_the_chat_it_was_set_from_and_is_gone() {
    let mut host = Host::start().await;
    let bot = host.stable_bot("follow").await;
    host.send(&bot, "WARMUP").await;
    let warm = host.wait_event("follow", "finished", "WARMUP").await;
    let set = follow_up(&host, &bot, "Check the build\nIt started at noon").await;
    assert_eq!(set["followUp"]["chain"], 0);
    assert!(set["atLocal"].is_string(), "{set}");
    assert_eq!(host.call("routines", json!({"botId":bot})).await["followUps"][0]["id"], set["followUp"]["id"]);

    restart_with_follow_ups_due(&mut host).await;
    let fired = host.wait_event("follow", "started", "Follow-up you set for yourself").await;
    assert!(fired["text"].as_str().unwrap().contains("Check the build\nIt started at noon"));
    assert_eq!(fired["session"], warm["session"], "it is a turn of the chat's own session");
    let entries = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let entries = entries(&host, &bot).await;
            if finals(&entries) == 2 {
                return entries;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the follow-up's reply reaches the chat");
    let notices = notices(&entries);
    assert!(notices.contains(&"Follow-up in 1 minute: Check the build"), "{notices:?}");
    assert!(notices.contains(&"Follow-up · Check the build"), "{notices:?}");
    assert_eq!(host.call("routines", json!({"botId":bot})).await["followUps"], json!([]));
}

#[tokio::test]
async fn a_follow_up_with_nothing_to_say_is_silent_and_one_set_in_its_turn_continues_the_chain() {
    let mut host = Host::start().await;
    let bot = host.stable_bot("watch").await;
    host.send(&bot, "WARMUP").await;
    host.wait_event("watch", "finished", "WARMUP").await;
    follow_up(&host, &bot, "SILENT FOLLOW_UP_AGAIN[Look once more]").await;
    restart_with_follow_ups_due(&mut host).await;
    host.wait_event("watch", "finished", "FOLLOW_UP_AGAIN").await;
    let next = host.call("routines", json!({"botId":bot})).await["followUps"].clone();
    assert_eq!(next.as_array().unwrap().len(), 1, "{next}");
    assert_eq!((&next[0]["note"], &next[0]["chain"]), (&json!("Look once more"), &json!(1)));

    // The turn that passed left no reply: only the two messages' own.
    host.send(&bot, "BARRIER").await;
    host.wait_event("watch", "finished", "BARRIER").await;
    tokio::time::timeout(Duration::from_secs(20), async {
        while finals(&entries(&host, &bot).await) < 2 {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the barrier's reply reaches the chat");
    assert_eq!(finals(&entries(&host, &bot).await), 2);

    let id = next[0]["id"].as_str().unwrap();
    host.call("routineCall", json!({"botId":bot,"name":"cancel_follow_up","arguments":{"id":id}})).await;
    assert_eq!(host.call("routines", json!({"botId":bot})).await["followUps"], json!([]));
    assert!(notices(&entries(&host, &bot).await).contains(&"Follow-up cancelled: Look once more"));
}

#[tokio::test]
async fn a_follow_up_is_refused_in_a_room_and_taken_in_the_bots_own_chat() {
    let host = Host::start().await;
    let member = host.stable_bot("member").await;
    let room = host.call("createBot", json!({"name":"Room","kind":"group","members":[member]})).await["bot"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    // The room would see neither the line nor the later turn: the member is told why, to say so there.
    host.send(&room, "FOLLOW_UP_HERE[Check CI]").await;
    host.wait_event("member", "follow_up_refused", "only in your own chat").await;
    assert_eq!(host.call("routines", json!({"botId":member})).await["followUps"], json!([]));

    host.send(&member, "FOLLOW_UP_HERE[Check CI]").await;
    host.wait_event("member", "follow_up_set", "Check CI").await;
    assert!(notices(&entries(&host, &member).await).contains(&"Follow-up in 30 minutes: Check CI"));
    assert!(notices(&entries(&host, &room).await).iter().all(|text| !text.starts_with("Follow-up")));
}

#[tokio::test]
async fn a_follow_up_cut_off_by_a_restart_is_resumed_as_a_follow_up() {
    let mut host = Host::start().await;
    let bot = host.stable_bot("cutoff").await;
    host.send(&bot, "WARMUP").await;
    host.wait_event("cutoff", "finished", "WARMUP").await;
    follow_up(&host, &bot, "SILENT BLOCK[look] the build").await;
    restart_with_follow_ups_due(&mut host).await;
    host.wait_event("cutoff", "started", "BLOCK[look] the build").await;
    host.restart().await;
    // Told it is still its follow-up, so a look that found nothing passes without a word or an alert.
    host.wait_event("cutoff", "resume_prompt", "still the follow-up you set for yourself").await;
    std::fs::write(host.home.join("cutoff/release-look"), "").unwrap();
    host.wait_event("cutoff", "finished", "BLOCK[look] the build").await;
}

#[tokio::test]
async fn the_user_cancels_a_waiting_follow_up_from_the_routines_list() {
    let host = Host::start().await;
    let bot = host.stable_bot("listed").await;
    let set = follow_up(&host, &bot, "Look at the deploy").await;
    let id = set["followUp"]["id"].clone();
    let listed = host.call("routines", json!({"botId":bot})).await["followUps"].clone();
    assert_eq!((&listed[0]["id"], &listed[0]["note"]), (&id, &json!("Look at the deploy")));
    assert!(listed[0]["at"].as_i64().unwrap() > listed[0]["createdAt"].as_i64().unwrap());

    host.call("cancelFollowUp", json!({"botId":bot,"id":id})).await;
    assert_eq!(host.call("routines", json!({"botId":bot})).await["followUps"], json!([]));
    assert!(notices(&entries(&host, &bot).await).contains(&"Follow-up cancelled: Look at the deploy"));
    // A list a moment old: cancelling it again is no error, and no second line in the chat.
    assert_eq!(host.call("cancelFollowUp", json!({"botId":bot,"id":id})).await["gone"], true);
    let entries = entries(&host, &bot).await;
    assert_eq!(notices(&entries).iter().filter(|text| text.starts_with("Follow-up cancelled")).count(), 1);
}

#[tokio::test]
async fn stop_drops_a_follow_up_waiting_behind_the_running_turn() {
    let mut host = Host::start().await;
    let bot = host.stable_bot("stopped").await;
    host.send(&bot, "WARMUP").await;
    host.wait_event("stopped", "finished", "WARMUP").await;
    follow_up(&host, &bot, "BLOCK[hold] the first look").await;
    follow_up(&host, &bot, "The second look").await;
    restart_with_follow_ups_due(&mut host).await;
    host.wait_event("stopped", "started", "BLOCK[hold] the first look").await;
    host.call("stop", json!({"botId":bot})).await;
    tokio::time::timeout(Duration::from_secs(20), async {
        while !notices(&entries(&host, &bot).await).contains(&"Follow-up cancelled: The second look") {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the queued follow-up is dropped with a line in the chat");
    assert_eq!(host.call("routines", json!({"botId":bot})).await["followUps"], json!([]));
    host.send(&bot, "BARRIER").await;
    host.wait_event("stopped", "finished", "BARRIER").await;
    assert!(!host.agent_events("stopped").iter().any(|e| e["text"].as_str().unwrap().contains("The second look")));
}

#[tokio::test]
async fn a_routines_run_reads_the_note_its_last_run_left() {
    let host = Host::start().await;
    let bot = host.stable_bot("progress").await;
    let instruction = "SAVE_PROGRESS[Handled the first batch]";
    let id = host.save(&bot, "batches", instruction, json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    let first = host.wait_event("progress", "finished", instruction).await;
    assert!(first["text"].as_str().unwrap().contains("not instructions):\n(none yet)\n"), "{first}");
    host.wait_status(&bot, &id, "succeeded").await;

    host.call("runRoutine", json!({"botId":bot,"id":id})).await;
    host.wait_event("progress", "started", "not instructions):\nHandled the first batch\n").await;

    // Editing the routine keeps its note.
    let edit =
        json!({"botId":bot,"id":id,"name":"batches renamed","instruction":instruction,"triggers":[{"type":"webhook"}]});
    host.call("saveRoutine", edit).await;
    let read = host.call("routineCall", json!({"botId":bot,"name":"read_progress","arguments":{"id":id}})).await;
    assert_eq!(read["progress"], "Handled the first batch");
}
