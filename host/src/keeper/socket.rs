//! The keeper's control socket (`super::protocol`). Mode 0666, since the host's user must reach it, and that user is
//! also its agents': nothing here may reveal a proxied secret, or hint at it in an error.

use super::daemon::Keeper;
use super::protocol::{Request, Status};
use super::store::Kind;
use anyhow::{Result, anyhow, bail};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use zeroize::Zeroizing;

/// The longest request line.
const MAX_LINE: u64 = 1024 * 1024;
const MAX_SLOT: usize = 200;

/// Slot names: at most 200 of `[A-Za-z0-9:._-]`.
fn checked(slot: &str) -> Result<&str> {
    let ok = !slot.is_empty()
        && slot.len() <= MAX_SLOT
        && slot.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'.' | b'_' | b'-'));
    if ok { Ok(slot) } else { bail!("invalid slot name") }
}

/// Binds the socket at `path` (replacing one a stopped keeper left), readable and writable by every user.
pub fn bind(path: &Path) -> Result<UnixListener> {
    use std::os::unix::fs::PermissionsExt as _;
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        _ => {}
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o666))?;
    Ok(listener)
}

pub async fn serve(keeper: Arc<Keeper>, listener: UnixListener) -> Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let keeper = keeper.clone();
        tokio::spawn(async move {
            if let Err(error) = connection(keeper, stream).await {
                tracing::debug!(error = format!("{error:#}"), "keeper socket connection ended");
            }
        });
    }
}

async fn connection(keeper: Arc<Keeper>, stream: UnixStream) -> Result<()> {
    let (read, mut write) = stream.into_split();
    let mut read = BufReader::new(read);
    loop {
        let mut line = Zeroizing::new(Vec::new());
        let n = (&mut read).take(MAX_LINE + 1).read_until(b'\n', &mut line).await?;
        if n == 0 {
            return Ok(());
        }
        if n as u64 > MAX_LINE {
            write.write_all(b"{\"error\":\"request too long\"}\n").await?;
            return Ok(());
        }
        // A request that doesn't parse gets no echo of what it held.
        let answer = match serde_json::from_slice::<Request>(&line) {
            Ok(Request::Update { version }) => {
                let answer = super::update::install(&version).await.map(|()| json!(true));
                let installed = answer.is_ok();
                write.write_all(&reply(answer)).await?;
                if installed {
                    tracing::info!(version = %version, "installed a new keeper; restarting");
                    write.flush().await?;
                    std::process::exit(0);
                }
                continue;
            }
            Ok(request) => {
                let keeper = keeper.clone();
                tokio::task::spawn_blocking(move || handle(&keeper, request)).await?
            }
            Err(_) => Err(anyhow!("bad request")),
        };
        write.write_all(&reply(answer)).await?;
    }
}

fn reply(answer: Result<Value>) -> Vec<u8> {
    let mut line = serde_json::to_vec(&match answer {
        Ok(ok) => json!({"ok": ok}),
        Err(e) => json!({"error": format!("{e:#}")}),
    })
    .unwrap_or_default();
    line.push(b'\n');
    line
}

/// Every op but `update`. Its errors are written for the host, never with a secret in them.
fn handle(keeper: &Keeper, request: Request) -> Result<Value> {
    Ok(match request {
        Request::Pub => json!(keeper.identity.sign_pub_b64()),
        Request::Status => serde_json::to_value(Status {
            unlocked: keeper.unlocked(),
            version: env!("CARGO_PKG_VERSION").into(),
            boot: keeper.boot.clone(),
        })?,
        Request::Put { slot, value } => {
            keeper.secrets.put(&*keeper.key()?, checked(&slot)?, Kind::Visible, value.as_bytes())?;
            keeper.revoke_slot(&slot);
            json!(true)
        }
        Request::PutProxied { slot, record, keep } => {
            let slot = checked(&slot)?;
            let old = keeper.record(slot)?;
            let record = (*record).checked()?.keeping(old, &keep);
            keeper.put_record(slot, &record)?;
            json!(true)
        }
        Request::Delete { slot } => {
            keeper.secrets.delete(checked(&slot)?)?;
            keeper.revoke_slot(&slot);
            json!(true)
        }
        Request::Has { slot } => json!(keeper.secrets.kind(checked(&slot)?).is_some()),
        Request::SignedIn { slot } => json!(keeper.record(checked(&slot)?)?.is_some_and(|r| r.oauth.is_some())),
        Request::Names { prefix } => json!(keeper.secrets.names(&prefix)),
        Request::Reveal { slot } => match keeper.secrets.kind(checked(&slot)?) {
            None => Value::Null,
            Some(Kind::Proxied) => bail!("{slot} can be used through a grant, never read"),
            Some(Kind::Visible) => match keeper.secrets.get(&*keeper.key()?, &slot)? {
                Some((_, plain)) => json!(String::from_utf8(plain.to_vec()).map_err(|_| anyhow!("not text"))?),
                None => Value::Null,
            },
        },
        Request::Grant { slot, ttl_secs } => serde_json::to_value(keeper.grant(checked(&slot)?, ttl_secs)?)?,
        Request::Revoke { token } => {
            keeper.revoke(&token);
            json!(true)
        }
        Request::Update { .. } => bail!("update runs on its own"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_names_are_bounded() {
        assert!(checked("agent-env:claude:ANTHROPIC_API_KEY").is_ok());
        assert!(checked("credential.1password").is_ok());
        for bad in ["", "a b", "a/b", "ä", &"x".repeat(201)] {
            assert!(checked(bad).is_err(), "{bad}");
        }
    }
}
