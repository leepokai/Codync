//! A routine whose answer ends in a parting `(pass)` says nothing; one that only mentions
//! the word still posts. Both runs are real host turns over ACP, as #506 happened, and both
//! places a run happens are covered: a task in the box and a turn in the chat.

use super::*;

/// Runs #506's shape on `bot` (named `name`), which must stay silent, then a reply that only mentions the word,
/// which must be posted.
async fn quiet_then_loud(host: &Host, bot: &str, name: &str) {
    // A line of reasoning first, then the pass in the same message.
    let quiet = host.save(bot, "quiet", "TRAILING_PASS[Product newsletter]", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":quiet})).await;
    // The turn really ran: the fake agent logged the prompt it was handed.
    host.wait_event(name, "started", "TRAILING_PASS").await;
    let run = host.wait_status(bot, &quiet, "succeeded").await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(finals(&entries(host, bot).await), 0, "a run that ends in a pass leaves the chat alone");

    // The same word inside a sentence is an ordinary answer, and it is posted.
    let loud = host.save(bot, "loud", "MENTIONS_PASS[Forward it]", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":loud})).await;
    host.wait_status(bot, &loud, "succeeded").await;
    common::eventually("an answer that only mentions a pass is posted", Duration::from_secs(15), async || {
        finals(&entries(host, bot).await) == 1
    })
    .await;
}

#[tokio::test]
async fn a_routine_that_ends_in_a_pass_posts_nothing_and_one_that_mentions_it_posts() {
    let host = Host::start().await;
    let bot = host.stable_bot("quiet").await;
    quiet_then_loud(&host, &bot, "quiet").await;
}

/// The other place a run happens: a bot that manages the team has no box (`chat::work::boxed` is
/// false), so its run is a turn in its chat and `active_routine` is what tells the rule it is a
/// routine. `stable_bot` above covers the box, whose thread is the `Source::Routine` half.
#[tokio::test]
async fn a_chat_routine_that_ends_in_a_pass_says_nothing_and_one_that_mentions_it_posts() {
    let host = Host::start().await;
    let bot = host.chat_run_bot("quiet-chat").await;
    quiet_then_loud(&host, &bot, "quiet-chat").await;
}

#[tokio::test]
async fn a_chat_routine_completion_names_the_saved_report_and_a_pass_stays_quiet() {
    let host = common::host::Host::start().await;
    let bot = host.fixture_bot("Chat routine", "routine_agent.py", "").await;
    host.works_in_chat(&bot).await;
    let mut events = host.events().await;
    let mut completions = Vec::new();

    for (name, instruction) in [("report", "REPORT_STATUS"), ("quiet", "TRAILING_PASS[Product newsletter]")] {
        let saved = host
            .call(
                "saveRoutine",
                json!({"botId":bot,"name":name,"instruction":instruction,"triggers":[{"type":"webhook"}]}),
            )
            .await;
        host.call("runRoutine", json!({"botId":bot,"id":saved["routine"]["id"]})).await;
        let finished = tokio::time::timeout(Duration::from_secs(25), async {
            let mut working = false;
            loop {
                let event = events.recv().await.unwrap();
                let row = &event["bot"];
                if event["type"] == "bot" && row["id"] == bot {
                    if working && row["status"] == "idle" {
                        break event["bot"].clone();
                    }
                    working |= row["status"] == "working";
                }
            }
        })
        .await
        .unwrap_or_else(|_| panic!("missing live completion for {name}"));
        completions.push(finished.get("completionReplyId").unwrap().clone());
    }

    let history = host.call("history", json!({"botId":bot})).await;
    let replies: Vec<_> = history["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "agent" && e["data"]["final"] == true)
        .collect();
    assert_eq!(replies.len(), 1, "only the report is visible in the chat");
    assert_eq!(replies[0]["data"]["text"], "Routine completed successfully.");
    assert_eq!(completions, [replies[0]["id"].clone(), Value::Null]);
}
