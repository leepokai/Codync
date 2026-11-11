//! Caller-owned local speech requests; cancellation also covers a not-yet-started prefetch.
use crate::{
    LockExt,
    agent::hermes::{Installation, Settings},
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};

struct Stream {
    receiver: tokio::sync::Mutex<mpsc::Receiver<Value>>,
    error: Mutex<Option<String>>,
}

struct Slot {
    cancel: watch::Sender<bool>,
    active: bool,
    touched: Instant,
    stream: Option<Arc<Stream>>,
}
static REQUESTS: LazyLock<Mutex<HashMap<(String, String), Slot>>> = LazyLock::new(|| Mutex::new(HashMap::new()));
fn prune(all: &mut HashMap<(String, String), Slot>) {
    all.retain(|_, s| s.active || s.touched.elapsed() < Duration::from_secs(120));
}
struct Pending((String, String));
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(s) = REQUESTS.locked().get_mut(&self.0) {
            s.active = false;
            s.touched = Instant::now();
        }
    }
}
fn key(caller: &str, id: &str) -> Result<(String, String)> {
    uuid::Uuid::parse_str(id).context("Speech requestId must be a UUID")?;
    Ok((caller.into(), id.into()))
}
pub(crate) fn cancel(caller: &str, id: &str) -> Result<Value> {
    let key = key(caller, id)?;
    let mut all = REQUESTS.locked();
    prune(&mut all);
    if let Some(s) = all.get_mut(&key) {
        s.cancel.send_replace(true);
        s.touched = Instant::now();
    } else {
        ensure!(all.len() < 256, "Too many recent speech requests");
        let (cancel, _) = watch::channel(true);
        all.insert(key, Slot { cancel, active: false, touched: Instant::now(), stream: None });
    }
    Ok(json!({"cancelled":true}))
}
fn begin(caller: &str, id: &str) -> Result<(Pending, watch::Receiver<bool>)> {
    let key = key(caller, id)?;
    let mut all = REQUESTS.locked();
    prune(&mut all);
    ensure!(!all.contains_key(&key), "Speech request was already completed, cancelled or started");
    ensure!(all.len() < 256 && all.values().filter(|s| s.active).count() < 8, "Too many pending speech requests");
    let (cancel, rx) = watch::channel(false);
    all.insert(key.clone(), Slot { cancel, active: true, touched: Instant::now(), stream: None });
    Ok((Pending(key), rx))
}
async fn installation(store: &Store, bot: Option<&str>) -> Result<Installation> {
    let config = match bot {
        Some(bot) => {
            store.bot(bot)?.filter(|b| !b.deleted).context("Unknown speech bot")?;
            crate::agent::hermes::load(store, bot)?.map(|s| s.config).unwrap_or_default()
        }
        None => Settings::default(),
    };
    Installation::resolve(&config).await
}
pub(crate) async fn speak(store: &Store, caller: &str, id: &str, bot: Option<&str>, text: &str) -> Result<Value> {
    let (_pending, mut cancelled) = begin(caller, id)?;
    tokio::select! { result=tokio::time::timeout(Duration::from_secs(80),async { let install=installation(store,bot).await?;super::neutts::speak(&install,text).await })=>result.context("Local speech timed out")?,
    _=cancelled.changed()=>anyhow::bail!("Local speech was cancelled") }
}
pub(crate) async fn status(store: &Store, bot: Option<&str>) -> Value {
    match installation(store, bot).await {
        Ok(install) => match super::neutts::ready(&install) {
            Ok(ready) => {
                json!({"engine":"neutts","prerequisitesAvailable":ready,"voice":"Jo","localOnly":true,"setup":"Run hermes setup tts and choose NeuTTS; prepare its models before calling."})
            }
            Err(e) => json!({"engine":"neutts","prerequisitesAvailable":false,"error":e.to_string()}),
        },
        Err(e) => json!({"engine":"neutts","prerequisitesAvailable":false,"error":e.to_string()}),
    }
}

pub(crate) fn start(
    hub: &Arc<crate::hub::Hub>,
    caller: &str,
    id: &str,
    bot: Option<&str>,
    text: &str,
) -> Result<Value> {
    ensure!(!text.trim().is_empty() && text.len() <= 4000 && !text.contains('\0'), "Invalid speech text");
    let (pending, mut cancelled) = begin(caller, id)?;
    let (sender, receiver) = mpsc::channel(3);
    let completion = sender.clone();
    let stream = Arc::new(Stream { receiver: tokio::sync::Mutex::new(receiver), error: Mutex::new(None) });
    REQUESTS.locked().get_mut(&pending.0).context("Speech request disappeared")?.stream = Some(stream.clone());
    let (hub, bot, text) = (hub.clone(), bot.map(str::to_owned), text.to_owned());
    tokio::spawn(async move {
        let _pending = pending;
        let result = tokio::select! {
            result=async { tokio::time::timeout(Duration::from_secs(80),async {
                let installation=installation(&hub.store,bot.as_deref()).await?;
                super::streaming::produce(&installation,&text,sender).await
            }).await.context("Local streaming speech timed out")? }=>result,
            _=cancelled.changed()=>Err(anyhow::anyhow!("Local speech was cancelled")),
        };
        if let Err(error) = result {
            *stream.error.locked() = Some(error.to_string());
        }
        drop(completion);
    });
    Ok(json!({"requestId":id,"format":"wav","sampleRate":24000,"streaming":true}))
}

pub(crate) async fn next(caller: &str, id: &str) -> Result<Value> {
    let (stream, mut cancelled) = {
        let all = REQUESTS.locked();
        let slot = all.get(&key(caller, id)?).context("Unknown or expired speech stream")?;
        ensure!(!*slot.cancel.borrow(), "Local speech was cancelled");
        (slot.stream.clone().context("This speech request is not a stream")?, slot.cancel.subscribe())
    };
    let mut receiver = stream.receiver.try_lock().context("A speech frame is already being read")?;
    let frame = tokio::select! {
        biased;
        _=cancelled.changed()=>anyhow::bail!("Local speech was cancelled"),
        frame=tokio::time::timeout(Duration::from_secs(85),receiver.recv())=>frame.context("Speech frame timed out")?,
    };
    if let Some(frame) = frame {
        return Ok(frame);
    }
    if let Some(error) = stream.error.locked().as_ref() {
        anyhow::bail!("{error}");
    }
    Ok(json!({"done":true}))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_is_caller_scoped_and_pre_cancelled_work_cannot_start() {
        let id = uuid::Uuid::new_v4().to_string();
        cancel("one", &id).unwrap();
        assert!(begin("one", &id).is_err());
        let (pending, mut rx) = begin("two", &id).unwrap();
        assert!(!*rx.borrow_and_update());
        cancel("one", &id).unwrap();
        assert!(!*rx.borrow_and_update());
        cancel("two", &id).unwrap();
        assert!(*rx.borrow_and_update());
        drop(pending);
        assert!(begin("two", &id).is_err());
    }
    #[tokio::test]
    async fn completed_stream_drains_before_terminal_error_and_is_private() {
        let id = uuid::Uuid::new_v4().to_string();
        let (pending, _) = begin("stream-owner", &id).unwrap();
        let (tx, rx) = mpsc::channel(3);
        let stream = Arc::new(Stream {
            receiver: tokio::sync::Mutex::new(rx),
            error: Mutex::new(Some("inference stopped".into())),
        });
        REQUESTS.locked().get_mut(&pending.0).unwrap().stream = Some(stream);
        tx.send(json!({"audio":"first","done":false})).await.unwrap();
        drop(tx);
        drop(pending);
        assert!(next("someone-else", &id).await.is_err());
        assert_eq!(next("stream-owner", &id).await.unwrap()["audio"], "first");
        assert!(next("stream-owner", &id).await.unwrap_err().to_string().contains("inference stopped"));
    }
}
