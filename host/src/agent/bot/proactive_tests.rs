//! Actor regressions with a local ACP fixture: no provider, key or network access.

use crate::api::apps_runtime::dispatch_bot_turn;
use crate::hub::{BotStatus, Hub};
use crate::store::{BotConfig, Entry, Store, is_internal};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const AGENT: &str = r"
import json, pathlib, sys, threading, time
sys.stdin.reconfigure(encoding='utf-8')
sys.stdout.reconfigure(encoding='utf-8')
lock = threading.Lock()
cancel = threading.Event()
def send(value):
    with lock:
        print(json.dumps({'jsonrpc':'2.0', **value}), flush=True)
def update(kind, text):
    send({'method':'session/update','params':{'sessionId':'session','update':{'sessionUpdate':kind,'content':{'type':'text','text':text}}}})
def prompt(request):
    text = request['params']['prompt'][0]['text']
    with open('prompts.jsonl','a',encoding='utf-8') as f:
        f.write(json.dumps(text)+'\n')
    scheduled = 'SCHEDULED_' in text
    if scheduled:
        update('agent_thought_chunk','Private reasoning')
        send({'method':'session/update','params':{'sessionId':'session','update':{'sessionUpdate':'tool_call','toolCallId':'read','title':'Read','kind':'read','status':'completed'}}})
    marker = 'silent' if 'SCHEDULED_SILENT' in text else 'useful' if scheduled else 'user' if 'HOLD_USER' in text else None
    first = 'NO_' if marker == 'silent' else 'Useful ' if scheduled else 'User '
    update('agent_message_chunk', first)
    while marker and not pathlib.Path('release-'+marker).exists() and not cancel.wait(0.01):
        pass
    update('agent_message_chunk', 'UPDATE' if marker == 'silent' else 'update' if scheduled else 'reply')
    send({'id':request['id'],'result':{'stopReason':'cancelled' if cancel.is_set() else 'end_turn'}})
for line in sys.stdin:
    request = json.loads(line)
    method = request.get('method')
    if method == 'initialize':
        send({'id':request['id'],'result':{'agentCapabilities':{'loadSession':True}}})
    elif method in ('session/new','session/load'):
        send({'id':request['id'],'result':{'sessionId':'session'}})
    elif method == 'session/prompt':
        cancel = threading.Event()
        threading.Thread(target=prompt,args=(request,),daemon=True).start()
    elif method == 'session/cancel':
        cancel.set()
    elif 'id' in request:
        send({'id':request['id'],'result':{}})
";

struct Fixture {
    hub: Arc<Hub>,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-proactive-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("agent.py");
        std::fs::write(&script, AGENT).unwrap();
        let store = Store::open(&dir.join("test.db")).unwrap();
        let cfg: BotConfig = serde_json::from_value(json!({
            "id":"check-in", "name":"Check-in", "backend":"fixture", "cwd":dir,
            "command":crate::shell::python(&script), "notify":false,
        }))
        .unwrap();
        store.save_bot(&cfg).unwrap();
        let hub = Hub::new(
            store,
            "fixture".into(),
            crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
            "fixture-token".into(),
            19223,
        );
        hub.start().unwrap();
        Self { hub, dir }
    }

    fn queue(&self, text: &str, from: &str) {
        dispatch_bot_turn(&self.hub, "check-in", text, from).unwrap();
    }

    fn raw(&self) -> Vec<Entry> {
        self.hub.store.entries_since(0, 1000).unwrap()
    }

    fn prompts(&self) -> Vec<String> {
        // The fixture appends while the test polls: a half-written last line waits for its next tick.
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
        .expect("scheduled fixture did not reach expected state");
    }

    fn release(&self, marker: &str) {
        std::fs::write(self.dir.join(format!("release-{marker}")), "").unwrap();
    }

    async fn wait_idle(&self) {
        self.until(|| self.hub.runtime("check-in").status == BotStatus::Idle).await;
    }

    async fn send_message(&self, text: &str) {
        let (reply, received) = tokio::sync::oneshot::channel();
        self.hub.send_cmd("check-in", super::Cmd::SendToUser { text: text.into(), reply }).unwrap();
        received.await.unwrap().unwrap();
    }

    async fn shutdown(self) {
        self.hub.shutdown().await;
    }
}

#[tokio::test]
async fn scheduled_no_update_never_becomes_visible_even_while_streaming_or_sent_explicitly() {
    let fixture = Fixture::new();
    let mut events = fixture.hub.events.subscribe();
    fixture.queue("SCHEDULED_SILENT", "proactive");
    fixture.until(|| fixture.raw().iter().any(|entry| entry.data["text"] == "NO_")).await;
    fixture.send_message(" NO_UPDATE\n").await;
    assert!(fixture.hub.store.history("check-in", i64::MAX, 100).unwrap().is_empty());
    assert!(fixture.hub.store.kv_get("turn.inflight.check-in").is_none_or(|value| value.is_empty()));
    assert!(fixture.hub.store.kv_get("turn.sources.check-in").is_none());
    fixture.release("silent");
    fixture.wait_idle().await;
    assert!(fixture.raw().iter().all(|entry| is_internal(&entry.data)));
    assert!(fixture.hub.store.last_message("check-in").is_none());
    assert_eq!(fixture.hub.store.unread("check-in", 0), 0);
    while let Ok(event) = events.try_recv() {
        if event["type"] == "entry" {
            assert!(is_internal(&event["entry"]["data"]), "a transient visible entry leaked: {event}");
        }
    }
    fixture.shutdown().await;
}

#[tokio::test]
async fn a_useful_scheduled_final_is_revealed_without_its_prompt_or_steps() {
    let fixture = Fixture::new();
    fixture.queue("SCHEDULED_USEFUL", "proactive");
    fixture.until(|| fixture.raw().iter().any(|entry| entry.data["text"] == "Useful ")).await;
    assert!(fixture.hub.store.history("check-in", i64::MAX, 100).unwrap().is_empty());
    fixture.release("useful");
    fixture.wait_idle().await;
    let public = fixture.hub.store.history("check-in", i64::MAX, 100).unwrap();
    assert_eq!(public.len(), 1);
    assert_eq!(public[0].data["text"], "Useful update");
    assert_eq!(fixture.hub.store.last_message("check-in").unwrap().text, "Useful update");
    assert_eq!(fixture.hub.store.unread("check-in", 0), 1);
    fixture.shutdown().await;
}

#[tokio::test]
async fn useful_explicit_sends_are_buffered_and_published_once_when_the_check_in_finishes() {
    let fixture = Fixture::new();
    fixture.queue("SCHEDULED_SILENT", "proactive");
    fixture.until(|| fixture.raw().iter().any(|entry| entry.data["text"] == "NO_")).await;
    fixture.send_message("The build passed").await;
    assert!(fixture.hub.store.history("check-in", i64::MAX, 100).unwrap().is_empty());
    fixture.release("silent");
    fixture.wait_idle().await;
    let public = fixture.hub.store.history("check-in", i64::MAX, 100).unwrap();
    assert_eq!(public.len(), 1);
    assert_eq!(public[0].data["text"], "The build passed");
    fixture.shutdown().await;
}

#[tokio::test]
async fn queued_check_ins_never_coalesce_with_a_persons_messages() {
    let fixture = Fixture::new();
    fixture.queue("HOLD_USER", "user");
    fixture.until(|| fixture.prompts().len() == 1).await;
    fixture.queue("SCHEDULED_SILENT", "proactive");
    fixture.queue("USER_FOLLOWUP", "user");
    fixture.release("user");
    fixture.until(|| fixture.prompts().len() == 2).await;
    assert!(!fixture.prompts()[1].contains("USER_FOLLOWUP"));
    fixture.release("silent");
    fixture.until(|| fixture.prompts().len() == 3).await;
    fixture.wait_idle().await;
    assert_eq!(fixture.prompts()[2], "USER_FOLLOWUP");
    let public = fixture.hub.store.history("check-in", i64::MAX, 100).unwrap();
    assert!(public.iter().any(|entry| entry.data["text"] == "USER_FOLLOWUP"));
    assert!(public.iter().all(|entry| entry.data["from"] != "proactive"));
    fixture.shutdown().await;
}
