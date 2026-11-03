//! JSON-RPC to the host over one connection; no request outlives it.

use anyhow::{Result, anyhow};
use serde_json::{Value, json};
use std::future::Future;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::task::JoinSet;

/// Messages to the host, one JSON line each.
pub type Outbox = mpsc::UnboundedSender<Value>;

pub async fn serve<S, F, R>(
    stream: S,
    out: Outbox,
    mut outbox: mpsc::UnboundedReceiver<Value>,
    handle: F,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Send + 'static,
    F: Fn(String, Value) -> R,
    R: Future<Output = Result<Value>> + Send + 'static,
{
    let (rd, mut wr) = tokio::io::split(stream);
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
    while requests.join_next().await.is_some() {}
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
        let reply = match response.await {
            Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
            Err(error) => json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32000, "message": format!("{error:#}")}}),
        };
        // A closed writer is handled by the enclosing connection loop.
        let _ = out.send(reply);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tokio::sync::Notify;

    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[tokio::test]
    async fn answers_requests_and_cancels_pending_ones_on_disconnect() {
        let (sock, mut host) = tokio::io::duplex(4096);
        let (out, outbox) = mpsc::unbounded_channel();
        let started = Arc::new(Notify::new());
        let dropped = Arc::new(AtomicBool::new(false));
        let serving = tokio::spawn(serve(sock, out, outbox, {
            let (started, dropped) = (started.clone(), dropped.clone());
            move |method, _| {
                let (started, dropped) = (started.clone(), dropped.clone());
                async move {
                    if method == "ping" {
                        return Ok(json!({"pong": true}));
                    }
                    let _guard = Dropped(dropped);
                    started.notify_one();
                    std::future::pending::<Result<Value>>().await
                }
            }
        }));
        host.write_all(b"{\"id\":1,\"method\":\"ping\",\"params\":{}}\n")
            .await
            .unwrap();
        let mut lines = BufReader::new(&mut host).lines();
        let reply: Value =
            serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
        assert_eq!(reply["result"]["pong"], true);
        host.write_all(b"{\"id\":2,\"method\":\"answer\",\"params\":{}}\n")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(1), started.notified())
            .await
            .unwrap();
        drop(host);
        let ended = tokio::time::timeout(Duration::from_secs(1), serving)
            .await
            .unwrap()
            .unwrap();
        assert!(ended.is_ok());
        assert!(
            dropped.load(Ordering::SeqCst),
            "negotiation cannot continue after cleanup"
        );
    }
}
