//! Letters for computers that are away: kept, and sent once they answer again. The same retry
//! pass opens a connection to each member this computer has none open to.

use super::{Inner, locked};
use crate::wire::{Envelope, Request, Response};
use anyhow::{Result, bail};
use iroh::EndpointId;
use std::collections::HashSet;
use std::sync::{Arc, Weak};
use std::time::{Duration, Instant};

/// A letter for a computer that is away waits this long, then goes.
const KEEP_FOR: Duration = Duration::from_hours(24);
const MAX_WAITING: usize = 200;
const RETRY_EVERY: Duration = Duration::from_secs(30);
const RESYNC_EVERY: Duration = Duration::from_hours(6);

pub(super) struct Queued {
    to: EndpointId,
    envelope: Envelope,
    since: Instant,
}

impl Inner {
    pub(super) fn queue(&self, to: EndpointId, envelope: Envelope) -> Result<()> {
        let mut outbox = locked(&self.outbox);
        if outbox.len() >= MAX_WAITING {
            bail!("too many letters are waiting for computers that are away");
        }
        outbox.push(Queued { to, envelope, since: Instant::now() });
        Ok(())
    }

    /// Sends what waited for `id`, in order, now that it answered.
    pub(super) fn flush(self: Arc<Self>, id: EndpointId) {
        let waiting: Vec<Queued> = {
            let mut outbox = locked(&self.outbox);
            let (theirs, rest) = std::mem::take(&mut *outbox).into_iter().partition(|q| q.to == id);
            *outbox = rest;
            theirs
        };
        if waiting.is_empty() {
            return;
        }
        tokio::spawn(async move {
            let mut left = waiting.into_iter();
            while let Some(q) = left.next() {
                match self.call(id, &Request::Deliver { envelope: q.envelope.clone() }).await {
                    Ok(Response::Refused { reason }) => {
                        // ponytail: dropped with a log line; a delivery-report channel when the host needs it.
                        tracing::warn!(id = %q.envelope.id, reason, "a waiting letter was refused");
                    }
                    // Still starting there, or away again: it waits for the next answer.
                    Ok(Response::Later { .. }) | Err(_) => {
                        let mut outbox = locked(&self.outbox);
                        outbox.push(q);
                        outbox.extend(left);
                        return;
                    }
                    Ok(_) => {}
                }
            }
        });
    }

    /// Says hello to every computer letters wait for and every one with no open connection: one
    /// that gets through opens a connection and flushes that computer's letters.
    pub(super) async fn retry(self: &Arc<Self>) {
        let mut due: HashSet<EndpointId> = {
            let mut outbox = locked(&self.outbox);
            outbox.retain(|q| q.since.elapsed() < KEEP_FOR);
            outbox.iter().map(|q| q.to).collect()
        };
        due.extend(self.unconnected());
        self.hello_each(due).await;
    }
}

/// Letters for computers that were away go out when those answer; this asks them now and then,
/// and every computer this one has no open connection to, so a computer whose firewall lets in
/// only what it started opens one the others then write over. And every few hours it compares
/// rosters with everyone, so a removal reaches a computer nobody happened to write to.
pub(super) async fn retry_loop(inner: Weak<Inner>) {
    let mut synced = Instant::now();
    loop {
        tokio::time::sleep(RETRY_EVERY).await;
        let Some(inner) = inner.upgrade() else { return };
        if synced.elapsed() >= RESYNC_EVERY {
            synced = Instant::now();
            inner.clone().hello_all();
        }
        inner.retry().await;
    }
}
