//! Requests belong to one host socket; none may outlive that connection.

use crate::Outbox;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::future::Future;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

pub(super) async fn serve<F, R>(
    sock: UnixStream,
    out: Outbox,
    mut outbox: mpsc::UnboundedReceiver<Value>,
    handle: F,
) -> Result<()>
where
    F: Fn(String, Value) -> R,
    R: Future<Output = Result<Value>> + Send + 'static,
{
    let (rd, mut wr) = sock.into_split();
    let mut writer = tokio::spawn(async move {
        while let Some(msg) = outbox.recv().await {
            wr.write_all(format!("{msg}\n").as_bytes()).await?;
        }
        Ok::<_, std::io::Error>(())
    });
    let mut requests = JoinSet::new();
    let mut lines = BufReader::new(rd).lines();
    let result = loop {
        tokio::select! {
            line = lines.next_line() => {
                let line = match line {
                    Ok(Some(line)) => line,
                    Ok(None) => break Ok(()),
                    Err(error) => break Err(error.into()),
                };
                dispatch(&line, &out, &handle, &mut requests);
            }
            Some(completed) = requests.join_next(), if !requests.is_empty() => {
                if let Err(error) = completed { tracing::warn!(%error, "screen request task failed"); }
            }
            completed = &mut writer => {
                break match completed {
                    Ok(Err(error)) => Err(error.into()),
                    Err(error) => Err(error.into()),
                    Ok(Ok(())) => Err(anyhow!("host writer disconnected")),
                };
            }
        }
    };
    requests.abort_all();
    loop {
        let completed = requests.join_next().await;
        if completed.is_none() {
            break;
        }
    }
    writer.abort();
    result
}

fn dispatch<F, R>(line: &str, out: &Outbox, handle: &F, requests: &mut JoinSet<()>)
where
    F: Fn(String, Value) -> R,
    R: Future<Output = Result<Value>> + Send + 'static,
{
    let Ok(msg) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let (Some(method), Some(id)) = (msg["method"].as_str(), msg.get("id").cloned()) else {
        return;
    };
    let response = handle(method.to_owned(), msg["params"].clone());
    let out = out.clone();
    requests.spawn(async move {
        let result = response.await;
        let reply = match result {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": format!("{error:#}")}}),
        };
        // A closed writer is handled by the enclosing connection loop.
        let _ = out.send(reply);
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    use std::time::Duration;
    use tokio::sync::Notify;

    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn disconnect_cancels_pending_answers_before_session_cleanup() {
        for invalid_line in [false, true] {
            let (sock, mut host) = UnixStream::pair().unwrap();
            let (out, outbox) = mpsc::unbounded_channel();
            let started = Arc::new(Notify::new());
            let dropped = Arc::new(AtomicBool::new(false));
            let serving = tokio::spawn(serve(sock, out, outbox, {
                let started = started.clone();
                let dropped = dropped.clone();
                move |_, _| {
                    let started = started.clone();
                    let dropped = dropped.clone();
                    async move {
                        let _guard = Dropped(dropped);
                        started.notify_one();
                        std::future::pending::<Result<Value>>().await
                    }
                }
            }));
            let sent = host
                .write_all(b"{\"id\":1,\"method\":\"answer\",\"params\":{}}\n")
                .await;
            sent.unwrap();
            let starting = tokio::time::timeout(Duration::from_secs(1), started.notified()).await;
            starting.unwrap();
            if invalid_line {
                let sent = host.write_all(b"\xff\n").await;
                sent.unwrap();
            } else {
                drop(host);
            }
            let ended = tokio::time::timeout(Duration::from_secs(1), serving).await;
            let result = ended.unwrap().unwrap();
            assert_eq!(result.is_err(), invalid_line);
            assert!(
                dropped.load(Ordering::SeqCst),
                "negotiation cannot continue after cleanup"
            );
        }
    }
}
