//! The queue above the composer: steering a waiting message into the running turn, editing and dropping one.

use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, Store};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Holds every prompt until `release` exists, so the second message waits in the queue where a test can reach it.
const AGENT: &str = r"
import json, pathlib, sys, threading
sys.stdin.reconfigure(encoding='utf-8')
sys.stdout.reconfigure(encoding='utf-8')
lock = threading.Lock()
def send(value):
    with lock:
        print(json.dumps({'jsonrpc':'2.0', **value}), flush=True)
def update(text):
    send({'method':'session/update','params':{'sessionId':'session','update':{'sessionUpdate':'agent_message_chunk','content':{'type':'text','text':text}}}})
def prompt(request):
    text = request['params']['prompt'][0]['text']
    with open('prompts.jsonl','a',encoding='utf-8') as f:
        f.write(json.dumps(text)+'\n')
    while not pathlib.Path('release').exists():
        pass
    update('reply')
    send({'id':request['id'],'result':{'stopReason':'end_turn'}})
for line in sys.stdin:
    request = json.loads(line)
    method = request.get('method')
    if method == 'initialize':
        send({'id':request['id'],'result':{'agentCapabilities':{'loadSession':True}}})
    elif method in ('session/new','session/load'):
        send({'id':request['id'],'result':{'sessionId':'session'}})
    elif method == 'session/prompt':
        threading.Thread(target=prompt,args=(request,),daemon=True).start()
    elif 'id' in request:
        send({'id':request['id'],'result':{}})
";

struct Fixture {
    hub: Arc<Hub>,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-queue-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("agent.py");
        std::fs::write(&script, AGENT).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let cfg: BotConfig = serde_json::from_value(json!({
            "id":"queue", "name":"Queue", "backend":"fixture", "cwd":dir,
            "command":crate::shell::python(&script), "notify":false,
        }))
        .unwrap();
        store.save_bot(&cfg).unwrap();
        let hub = Hub::new(
            store,
            "fixture".into(),
            crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
            "fixture-token".into(),
            19224,
        );
        hub.start().unwrap();
        Self { hub, dir }
    }

    /// The user writes in the chat, as the app does.
    fn post(&self, text: &str) -> Entry {
        let cfg = self.hub.store.bot("queue").unwrap().unwrap().config;
        crate::chat::messages::post(&self.hub, &cfg, json!({"text": text, "status": "queued"})).unwrap()
    }

    fn entry(&self, id: &str) -> Entry {
        self.hub.store.entry(id).unwrap()
    }

    fn prompts(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("prompts.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    async fn until(&self, ready: impl Fn() -> bool) {
        tokio::time::timeout(Duration::from_secs(30), async {
            while !ready() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("the fixture did not reach the expected state");
    }

    async fn steer(&self, entry_id: &str) -> anyhow::Result<()> {
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.hub.send_cmd("queue", super::Cmd::Steer { entry_id: entry_id.into(), reply }).unwrap();
        answer.await.unwrap()
    }

    async fn edit(&self, entry_id: &str, text: &str) -> anyhow::Result<()> {
        let (reply, answer) = tokio::sync::oneshot::channel();
        let cmd = super::Cmd::EditQueued { entry_id: entry_id.into(), text: text.into(), reply };
        self.hub.send_cmd("queue", cmd).unwrap();
        answer.await.unwrap()
    }

    async fn delete(&self, entry_id: &str) -> anyhow::Result<()> {
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.hub.send_cmd("queue", super::Cmd::DeleteQueued { entry_id: entry_id.into(), reply }).unwrap();
        answer.await.unwrap()
    }

    async fn shutdown(self) {
        self.hub.shutdown().await;
    }
}

#[tokio::test]
async fn a_steered_message_joins_the_running_turn_and_takes_no_turn_of_its_own() {
    let fixture = Fixture::new();
    fixture.post("first");
    fixture.until(|| fixture.prompts().len() == 1).await;
    let second = fixture.post("second");

    fixture.steer(&second.id).await.unwrap();
    assert_eq!(fixture.entry(&second.id).data["status"], "sent");
    fixture.until(|| fixture.prompts().len() == 2).await;
    assert_eq!(fixture.prompts()[1], "second");

    std::fs::write(fixture.dir.join("release"), "").unwrap();
    // One turn, not two: the steered prompt extends it and the queue stays empty.
    fixture.until(|| fixture.hub.runtime("queue").status == BotStatus::Idle).await;
    assert_eq!(fixture.prompts().len(), 2);
    assert!(fixture.hub.store.max_turn("queue") > 0);
    fixture.shutdown().await;
}

#[tokio::test]
async fn a_message_already_sent_cannot_be_steered() {
    let fixture = Fixture::new();
    let first = fixture.post("first");
    fixture.until(|| fixture.prompts().len() == 1).await;
    let error = fixture.steer(&first.id).await.unwrap_err().to_string();
    assert!(error.contains("already went out"), "{error}");
    std::fs::write(fixture.dir.join("release"), "").unwrap();
    fixture.shutdown().await;
}

#[tokio::test]
async fn a_waiting_message_can_be_rewritten_or_dropped_before_it_goes_out() {
    let fixture = Fixture::new();
    fixture.post("first");
    fixture.until(|| fixture.prompts().len() == 1).await;
    let edited = fixture.post("second");
    let dropped = fixture.post("third");

    fixture.edit(&edited.id, "rewritten").await.unwrap();
    assert_eq!(fixture.entry(&edited.id).data["text"], "rewritten");
    fixture.delete(&dropped.id).await.unwrap();
    assert_eq!(fixture.entry(&dropped.id).data["status"], "cancelled");

    std::fs::write(fixture.dir.join("release"), "").unwrap();
    fixture.until(|| fixture.prompts().len() == 2).await;
    // The agent hears the rewritten text; the dropped one never reaches it.
    assert_eq!(fixture.prompts()[1], "rewritten");
    assert_eq!(fixture.prompts().len(), 2);
    fixture.shutdown().await;
}
