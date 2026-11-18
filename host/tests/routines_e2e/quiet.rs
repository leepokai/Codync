//! A routine whose answer ends in a parting `(pass)` says nothing; one that only mentions
//! the word still posts. Both runs are real host turns over ACP, as #506 happened, and both
//! places a run happens are covered: an item in the box and a turn in the chat.

use super::*;

fn finals(entries: &[Value]) -> usize {
    entries.iter().filter(|e| e["kind"] == "agent" && e["data"]["final"] == true).count()
}

async fn entries(host: &Host, bot: &str) -> Vec<Value> {
    host.call("history", json!({"botId":bot})).await["entries"].as_array().unwrap().clone()
}

#[tokio::test]
async fn a_routine_that_ends_in_a_pass_posts_nothing_and_one_that_mentions_it_posts() {
    let host = Host::start().await;
    let bot = host.stable_bot("quiet").await;

    // #506's shape: a line of reasoning first, then the pass in the same message.
    let quiet = host.save(&bot, "quiet", "TRAILING_PASS[Product newsletter]", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":quiet})).await;
    // The turn really ran: the fake agent logged the prompt it was handed.
    host.wait_event("quiet", "started", "TRAILING_PASS").await;
    let run = host.wait_status(&bot, &quiet, "succeeded").await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(finals(&entries(&host, &bot).await), 0, "a run that ends in a pass leaves the chat alone");

    // The same word inside a sentence is an ordinary answer, and it is posted.
    let loud = host.save(&bot, "loud", "MENTIONS_PASS[Forward it]", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":loud})).await;
    host.wait_status(&bot, &loud, "succeeded").await;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if finals(&entries(&host, &bot).await) == 1 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("an answer that only mentions a pass is still a result");
}

/// The other place a run happens: a bot that manages the team has no box (`chat::tasks::boxed` is
/// false), so its run is a turn in its chat and `active_routine` is what tells the rule it is a
/// routine. `stable_bot` above covers the box, whose task is the `Source::Routine` half.
#[tokio::test]
async fn a_chat_routine_that_ends_in_a_pass_says_nothing_and_one_that_mentions_it_posts() {
    let host = Host::start().await;
    let bot = host.chat_run_bot("quiet-chat").await;

    let quiet = host.save(&bot, "quiet", "TRAILING_PASS[Inbox empty]", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":quiet})).await;
    host.wait_event("quiet-chat", "started", "TRAILING_PASS").await;
    let run = host.wait_status(&bot, &quiet, "succeeded").await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(finals(&entries(&host, &bot).await), 0, "a chat routine that ends in a pass leaves the chat alone");

    let loud = host.save(&bot, "loud", "MENTIONS_PASS[Forward it]", json!([{"type":"webhook"}]), 30).await;
    host.call("runRoutine", json!({"botId":bot,"id":loud})).await;
    host.wait_status(&bot, &loud, "succeeded").await;
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if finals(&entries(&host, &bot).await) == 1 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("an answer that only mentions a pass is still a result in the chat too");
}
