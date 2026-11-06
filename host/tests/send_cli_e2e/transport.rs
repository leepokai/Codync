//! Controlled transport failures and event ordering that a fast real host cannot guarantee.

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response, Sse, sse::Event};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::{Stream, StreamExt};
use serde_json::{Value, json};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    DroppedResponse,
    EarlyFinal,
    Rejected,
    Resync,
    PriorApproval,
    /// Waiting for an approval whose card is older than the catch-up.
    ApprovalOutsideCatchUp,
}

#[derive(Clone)]
struct Shared {
    mode: Mode,
    token: String,
    requests: Arc<Mutex<Vec<Value>>>,
    events: broadcast::Sender<Value>,
}

pub struct MockHost {
    pub port: String,
    state: Shared,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for MockHost {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl MockHost {
    pub async fn start(mode: Mode, token: &str) -> Self {
        let (events, _) = broadcast::channel(32);
        let state = Shared { mode, token: token.to_owned(), requests: Arc::new(Mutex::new(Vec::new())), events };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port().to_string();
        let app =
            Router::new().route("/events", get(events_stream)).route("/api/send", post(send)).with_state(state.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self { port, state, task }
    }

    pub fn requests(&self) -> Vec<Value> {
        self.state.requests.lock().unwrap().clone()
    }
}

fn bot(status: &str, rev: i64) -> Value {
    json!({"type":"bot","rev":rev,"bot":{"id":"transport-bot","name":"Transport","rev":rev,"deleted":false,"status":status}})
}

fn entry(status: &str, rev: i64) -> Value {
    json!({"id":"message","botId":"transport-bot","seq":2,"turn":1,"kind":"user","rev":rev,"data":{"status":status}})
}

fn reply(id: &str, seq: i64, text: &str, rev: i64) -> Value {
    let reply = json!({"id":id,"botId":"transport-bot","seq":seq,"turn":1,"kind":"agent","rev":rev,"data":{"final":true,"text":text}});
    json!({"type":"entry","rev":rev,"entry":reply})
}

fn permission(status: &str, rev: i64) -> Value {
    json!({"type":"entry","rev":rev,"entry":{"id":"prior-approval","botId":"transport-bot","seq":1,"turn":0,"kind":"permission","rev":rev,"data":{"status":status,"title":"Approve earlier work"}}})
}

async fn events_stream(State(state): State<Shared>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let receiver = state.events.subscribe();
    // `rev` is ahead of the catch-up, as after a write no event carries (a memory file, say).
    let head = if state.mode == Mode::PriorApproval {
        vec![json!({"type":"hello","rev":9,"catchUp":2}), bot("needsInput", 1), permission("pending", 2)]
    } else if state.mode == Mode::ApprovalOutsideCatchUp {
        vec![json!({"type":"hello","rev":9,"catchUp":1}), bot("needsInput", 1)]
    } else {
        vec![json!({"type":"hello","rev":9,"catchUp":1}), bot("idle", 1)]
    };
    let head = futures::stream::iter(head);
    let tail = BroadcastStream::new(receiver).filter_map(|event| async move { event.ok() });
    Sse::new(head.chain(tail).map(|event| Ok(Event::default().data(event.to_string()))))
}

async fn send(State(state): State<Shared>, Json(body): Json<Value>) -> Response {
    let attempt = {
        let mut requests = state.requests.lock().unwrap();
        requests.push(body);
        requests.len()
    };
    if state.mode == Mode::Rejected {
        return (StatusCode::FORBIDDEN, Json(json!({"error":format!("denied {}", state.token)}))).into_response();
    }
    if state.mode == Mode::DroppedResponse && attempt == 1 {
        let broken =
            futures::stream::once(async { Err::<Bytes, _>(std::io::Error::other("fixture dropped response")) });
        return Response::new(Body::from_stream(broken));
    }
    if state.mode == Mode::Resync {
        let _ = state.events.send(json!({"type":"resync"}));
    }
    if matches!(state.mode, Mode::EarlyFinal | Mode::PriorApproval | Mode::ApprovalOutsideCatchUp) {
        tokio::spawn(async move {
            if matches!(state.mode, Mode::PriorApproval | Mode::ApprovalOutsideCatchUp) {
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            if state.mode == Mode::PriorApproval {
                let _ = state.events.send(permission("approved", 3));
            }
            let _ = state.events.send(json!({"type":"entry","rev":3,"entry":entry("queued", 3)}));
            let _ = state.events.send(json!({"type":"entry","rev":4,"entry":entry("sent", 4)}));
            // A reaction to an older message re-sends it as `sent`: it mustn't take over the waited turn.
            let older = json!({"id":"older","botId":"transport-bot","seq":1,"turn":0,"kind":"user","rev":4,"data":{"status":"sent","reaction":"👍"}});
            let _ = state.events.send(json!({"type":"entry","rev":4,"entry":older}));
            let _ = state.events.send(bot("working", 5));
            let _ = state.events.send(reply("first", 3, "Early final", 6));
            if state.mode == Mode::EarlyFinal {
                tokio::time::sleep(Duration::from_millis(1200)).await;
                // A second message of the same reply (send_message).
                let _ = state.events.send(reply("second", 4, "Second bubble", 7));
            }
            let _ = state.events.send(bot("idle", 8));
        });
    }
    Json(json!({"entry":entry("queued", 2)})).into_response()
}
