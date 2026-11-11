use super::{config, ingress, outgoing, webhook};
use crate::{
    LockExt,
    agent::{
        hermes::{Installation, Settings},
        process::Process,
    },
    hub::Hub,
    market::Connector,
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
    task::JoinHandle,
};

const MAX_FRAME: usize = 512 * 1024;
struct Tasks(Vec<JoinHandle<()>>);
impl Drop for Tasks {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

pub(crate) async fn run(hub: Arc<Hub>) {
    let mut workers: HashMap<String, (String, JoinHandle<()>)> = HashMap::new();
    loop {
        let unlocked = { hub.store.secret_key.locked().is_some() };
        if unlocked {
            match crate::market::connectors(&hub.store) {
                Ok(all) => {
                    let channels = all.into_iter().filter(super::is_native).collect::<Vec<_>>();
                    let mut stopped = Vec::new();
                    workers.retain(|id, (base, task)| {
                        let keep = !task.is_finished()
                            && channels.iter().any(|c| &c.id == id && config::fingerprint(c) == *base);
                        if !keep {
                            task.abort();
                            let generation = config::state(&hub.store, id)
                                .ok()
                                .flatten()
                                .and_then(|s| s["generation"].as_str().map(str::to_owned));
                            if let Err(error) = super::revoke(&hub, id, generation, None) {
                                tracing::warn!(%error,"Native connection revocation was deferred");
                            }
                        }
                        // Await Tokio cancellation completion before admitting a replacement transport.
                        let retained = keep || !task.is_finished();
                        if !retained {
                            stopped.push(id.clone());
                        }
                        retained
                    });
                    for id in stopped {
                        if let Err(error) = super::media::cleanup_unknown(&hub.store, &id).await {
                            tracing::warn!(%error,"Native disconnected snapshot cleanup was deferred");
                        }
                    }
                    for c in channels {
                        if workers.contains_key(&c.id) {
                            continue;
                        }
                        let id = c.id.clone();
                        let base = config::fingerprint(&c);
                        let hub = hub.clone();
                        let task = tokio::spawn(async move {
                            let mut delay = 2;
                            loop {
                                if !config::matches(&hub.store, &c).unwrap_or(false) {
                                    return;
                                }
                                let result = serve(&hub, &c).await;
                                if result.is_err() {
                                    let generation =
                                        config::generation(&hub.store, &c).ok().flatten().unwrap_or_default();
                                    if let Err(error) = config::save_state(&hub.store, &c, &generation, "error") {
                                        tracing::warn!(%error,"Could not save native channel failure");
                                    }
                                    tracing::warn!(connection=%c.id,"Native channel needs attention; no ambiguous sends will be repeated");
                                }
                                tokio::time::sleep(Duration::from_secs(delay)).await;
                                delay = (delay * 2).min(60);
                            }
                        });
                        workers.insert(id, (base, task));
                    }
                }
                Err(error) => tracing::warn!(%error,"Native channel configuration could not be read"),
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn publish() -> Result<PathBuf> {
    let files = [
        ("HERMES-LICENSE", include_str!("../market/hermes/LICENSE")),
        ("bridge.py", include_str!("bridge.py")),
        ("native_profile.py", include_str!("../agent/hermes/acp_owner.py")),
        ("homeassistant/__init__.py", include_str!("homeassistant/__init__.py")),
        ("homeassistant/adapter.py", include_str!("homeassistant/adapter.py")),
        ("homeassistant/tools.py", include_str!("homeassistant/tools.py")),
        ("homeassistant/LICENSE", include_str!("homeassistant/LICENSE")),
    ];
    let mut digest = Sha256::new();
    for (name, text) in &files {
        digest.update(name.as_bytes());
        digest.update(text.as_bytes());
    }
    let root = crate::service::data_dir()
        .join("native-channel-runtime")
        .join(crate::chat::files::digest_hex(&digest.finalize()));
    for (name, text) in files {
        let path = root.join(name);
        tokio::fs::create_dir_all(path.parent().context("Bridge directory is missing")?).await?;
        if tokio::fs::read(&path).await.ok().as_deref() == Some(text.as_bytes()) {
            continue;
        }
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        tokio::fs::write(&temporary, text).await?;
        if let Err(error) = tokio::fs::rename(&temporary, &path).await {
            let equal = tokio::fs::read(&path).await.ok().as_deref() == Some(text.as_bytes());
            let _ = tokio::fs::remove_file(&temporary).await;
            ensure!(equal, "Could not publish native platform adapter: {error}");
        }
    }
    Ok(root.join("bridge.py"))
}

async fn write(input: &mut tokio::process::ChildStdin, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    ensure!(bytes.len() <= MAX_FRAME, "Native transport frame is too large");
    bytes.push(b'\n');
    tokio::time::timeout(Duration::from_secs(5), input.write_all(&bytes))
        .await
        .context("Native transport input timed out")??;
    Ok(())
}

async fn read_frames(output: tokio::process::ChildStdout, sender: mpsc::Sender<Result<Value>>) {
    let mut reader = BufReader::new(output);
    loop {
        let result = async {
            let mut bytes = Vec::new();
            loop {
                let available = reader.fill_buf().await?;
                if available.is_empty() {
                    return anyhow::Ok(None);
                }
                let take = available.iter().position(|b| *b == b'\n').map_or(available.len(), |n| n + 1);
                ensure!(bytes.len() + take <= MAX_FRAME, "Native transport output exceeded its limit");
                bytes.extend_from_slice(&available[..take]);
                reader.consume(take);
                if bytes.last() == Some(&b'\n') {
                    break;
                }
            }
            anyhow::Ok(Some(serde_json::from_slice::<Value>(&bytes)?))
        }
        .await;
        match result {
            Ok(Some(frame)) => {
                if sender.send(Ok(frame)).await.is_err() {
                    return;
                }
            }
            Ok(None) => return,
            Err(error) => {
                let _ = sender.send(Err(error)).await;
                return;
            }
        }
    }
}

async fn processors(
    hub: Arc<Hub>,
    c: Connector,
    generation: String,
    cache: PathBuf,
    sends: mpsc::Sender<Value>,
    outbound: bool,
) {
    loop {
        let active = config::state(&hub.store, &c.id)
            .ok()
            .flatten()
            .is_some_and(|s| s["status"] == "ready" && s["generation"] == generation);
        if active {
            let result = if outbound {
                outgoing::queue(&hub, &c, &generation, &sends).await
            } else {
                ingress::deliver(&hub, &c, &generation, &cache).await
            };
            if result.is_err() {
                tracing::warn!(connection=%c.id,"Native queued work was deferred");
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn serve(hub: &Arc<Hub>, c: &Connector) -> Result<()> {
    super::media::cleanup_unknown(&hub.store, &c.id).await?;
    hub.store.with_connection(|db| {
        db.execute(
            "UPDATE native_channel_outbox SET state='unknown' WHERE connection=?1 AND state='sending'",
            [&c.id],
        )?;
        db.execute(
            "UPDATE native_channel_webhooks SET state='queued' WHERE connection=?1 AND state='claimed'",
            [&c.id],
        )?;
        Ok(())
    })?;
    let platform = config::kind(c).context("Unknown native platform")?;
    let mut settings = Settings::default();
    settings.root = c.env.get("CODYNC_CHANNEL_ROOT").cloned().unwrap_or_default();
    settings.python = c.env.get("CODYNC_CHANNEL_PYTHON").cloned().unwrap_or_default();
    let old = config::generation(&hub.store, c)?.unwrap_or_default();
    config::save_state(&hub.store, c, &old, "starting")?;
    let installation = Installation::resolve(&settings).await?;
    let bridge = publish().await?;
    let cache = crate::service::data_dir().join("native-channel-media").join(&c.id).join(config::fingerprint(c));
    tokio::fs::create_dir_all(&cache).await?;
    let mut command = tokio::process::Command::new(&installation.python);
    command
        .arg(bridge)
        .arg("--root")
        .arg(&installation.root)
        .arg("--kind")
        .arg(platform)
        .arg("--cache")
        .arg(&cache)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    Process::prepare(&mut command);
    #[cfg(windows)]
    command.creation_flags(
        windows_sys::Win32::System::Threading::CREATE_SUSPENDED
            | windows_sys::Win32::System::Threading::CREATE_NO_WINDOW,
    );
    let mut child = command.spawn()?;
    let mut input = child.stdin.take().context("Native channel input is unavailable")?;
    let output = child.stdout.take().context("Native channel output is unavailable")?;
    let mut owned = Process::new(child)?;
    let (frames, mut received) = mpsc::channel(16);
    let (sends, mut outgoing) = mpsc::channel(32);
    let mut tasks = Tasks(vec![tokio::spawn(read_frames(output, frames))]);
    write(&mut input, &json!({"op":"init","config":c.env})).await?;
    let mut generation = String::new();
    let mut identity = false;
    let mut started = false;
    let mut bound_bot = config::bot(hub, c).ok();
    let mut tick = tokio::time::interval(Duration::from_secs(2));
    let mut last = tokio::time::Instant::now();
    let result=async {
    loop {
        tokio::select! {
            frame=received.recv()=>{
                let frame=frame.context("Native transport closed")??;last=tokio::time::Instant::now();
                match frame["op"].as_str() {
                    Some("identity")=>{
                        ensure!(!identity,"Native transport changed identity without reconnecting");
                        let native=frame["identity"].as_str().filter(|s|s.len()==64 && s.bytes().all(|b|b.is_ascii_hexdigit())).context("Native account identity is missing")?;
                        generation=config::digest(&format!("{}:{native}",config::fingerprint(c)));identity=true;
                        let _guard=hub.store.connector_lock.locked();ensure!(config::matches(&hub.store,c)?,"Native connection changed");
                        if !old.is_empty() && old!=generation {super::revoke(hub,&c.id,Some(old.clone()),None)?;}
                        hub.store.with_connection(|db| {
                            db.execute("UPDATE native_channel_webhooks SET state='done',raw='' WHERE connection=?1 AND generation<>?2",rusqlite::params![c.id,generation])?;
                            Ok(())
                        })?;
                        let users=frame["nativeUsers"].as_array().context("Native sender policy is missing")?;
                        ensure!(users.len()<=200 && users.iter().all(|u|u.as_str().is_some_and(|u|u.len()<=512)),"Native sender policy is invalid");
                        hub.store.kv_set(&format!("native-channel.state:{}",c.id),&json!({"base":config::fingerprint(c),"generation":generation,"status":"connecting","nativeUsers":users,"updatedAt":crate::store::now_ms()}).to_string())?;
                        if let Some(code)=c.env.get("CODYNC_CHANNEL_PAIR_CODE").filter(|s|!s.is_empty()) {
                            hub.store.native_seed_pairing(&c.id,&generation,&config::digest(code),crate::store::now_ms()+10*60*1000)?;
                        }
                    }
                    Some("ready")=>{
                        ensure!(identity,"Native account was not identified");config::save_state(&hub.store,c,&generation,if config::bot(hub,c).is_ok(){"ready"}else{"needs_bot"})?;
                        if !started {
                            tasks.0.push(tokio::spawn(processors(hub.clone(),c.clone(),generation.clone(),cache.clone(),sends.clone(),false)));
                            tasks.0.push(tokio::spawn(processors(hub.clone(),c.clone(),generation.clone(),cache.clone(),sends.clone(),true)));started=true;
                        }
                    }
                    Some("authorize"|"incoming")=>{
                        ensure!(identity,"Native account was not identified");
                        let answer=if frame["op"]=="authorize" {ingress::authorize(hub,c,&generation,&frame)}else{ingress::accept(hub,c,&generation,&frame["event"])};
                        let mut answer=answer.unwrap_or_else(|_|json!({"ok":false}));answer["op"]="answer".into();answer["id"]=frame["id"].clone();write(&mut input,&answer).await?;
                    }
                    Some("sent")=>{
                        if let Some(id)=frame["id"].as_str() {
                            hub.store.native_finish_send(id,frame["success"]==true,frame["messageId"].as_str())?;
                            if let Err(error)=super::media::cleanup_sent(&hub.store,id).await {tracing::warn!(%error,"Native transport snapshot cleanup was deferred");}
                        }
                    }
                    Some("health")=>if identity {config::save_state(&hub.store,c,&generation,if frame["connected"]!=true {"reconnecting"}else if config::bot(hub,c).is_ok(){"ready"}else{"needs_bot"})?;},
                    Some("webhookDone")=>if let Some(id)=frame["id"].as_str().filter(|id|id.len()==64 && id.bytes().all(|b|b.is_ascii_hexdigit())){
                        webhook::completed(&hub.store,id,frame["success"]==true)?;
                        if frame["success"]==true {let _=tokio::fs::remove_file(cache.join(format!("webhook-{id}.eml"))).await;}
                    },
                    Some("error")=>anyhow::bail!("Native platform failed to initialize; check its original account and dependencies"),
                    _=>anyhow::bail!("Unknown native transport frame"),
                }
            }
            Some(command)=outgoing.recv()=>{
                if outgoing::still_allowed(hub,c,&generation,&command)? {write(&mut input,&command).await?;}
                else if let Some(id)=command["id"].as_str(){hub.store.native_finish_send(id,false,None)?;super::media::cleanup_sent(&hub.store,id).await?;}
            }
            _=tick.tick()=>{
                if !config::matches(&hub.store,c)? {owned.kill().await;return Ok(())}
                let current_bot=config::bot(hub,c).ok();
                if current_bot!=bound_bot {
                    if identity {super::revoke(hub,&c.id,Some(generation.clone()),None)?;}
                    bound_bot=current_bot;
                }
                ensure!(last.elapsed()<Duration::from_secs(90),"Native channel stopped responding");
                if identity && started {for command in webhook::pending(&hub.store,c,&generation,&cache).await? {write(&mut input,&command).await?;}}
            }
        }
    }
    }.await;
    owned.kill().await;
    for task in &tasks.0 {
        task.abort();
    }
    for task in tasks.0.drain(..) {
        if let Err(error) = task.await {
            if !error.is_cancelled() {
                tracing::warn!(%error,"Native processor shutdown failed");
            }
        }
    }
    if let Err(error) = hub.store.with_connection(|db| {
        db.execute(
            "UPDATE native_channel_outbox SET state='unknown' WHERE connection=?1 AND state='sending'",
            [&c.id],
        )?;
        Ok(())
    }) {
        tracing::warn!(%error,"Native interrupted receipts could not be marked unknown");
    }
    if let Err(error) = super::media::cleanup_unknown(&hub.store, &c.id).await {
        tracing::warn!(%error,"Native ambiguous snapshot cleanup was deferred");
    }
    result
}
