//! What this computer answers: joins, hellos and letters for its agents.

use super::{Inner, check_name, locked};
use crate::guard::digest;
use crate::reach::Reach;
use crate::roster::Change;
use crate::types::Message;
use crate::wire::{self, Envelope, Request, Response};
use anyhow::{Result, anyhow, bail};
use iroh::EndpointId;
use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};
use std::fmt;
use std::sync::Arc;
use std::time::Instant;

/// Letters remembered as taken, for spotting a sender's retry.
const TAKEN_MAX: usize = 1000;

#[derive(Debug)]
pub(super) struct Handler(pub(super) Arc<Inner>);

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inner").field("me", &self.me().fmt_short().to_string()).finish_non_exhaustive()
    }
}

impl ProtocolHandler for Handler {
    async fn accept(&self, conn: Connection) -> Result<(), AcceptError> {
        let peer = conn.remote_id();
        while let Ok((send, recv)) = conn.accept_bi().await {
            let inner = self.0.clone();
            tokio::spawn(async move {
                if let Err(e) = serve(&inner, peer, send, recv).await {
                    tracing::debug!(peer = %peer.fmt_short(), error = format!("{e:#}"), "request failed");
                }
            });
        }
        Ok(())
    }
}

async fn serve(inner: &Arc<Inner>, peer: EndpointId, mut send: SendStream, mut recv: RecvStream) -> Result<()> {
    let request: Request = wire::read(&mut recv).await?;
    let response = inner.answer(peer, request).await;
    wire::answer(&mut send, &response).await
}

fn refuse(reason: impl Into<String>) -> Response {
    Response::Refused { reason: reason.into() }
}

impl Inner {
    async fn answer(self: &Arc<Self>, peer: EndpointId, request: Request) -> Response {
        if let Request::Join { network, secret, name } = request {
            return self.admit(peer, &network, &secret, &name).unwrap_or_else(|e| refuse(format!("{e:#}")));
        }
        let Ok(roster) = self.roster() else { return refuse("this computer isn't in a network") };
        if roster.is_removed(&peer) {
            return refuse("this computer was removed from the network");
        }
        if !roster.is_member(&peer) {
            return refuse("not a member of this network");
        }
        // It answers, so letters that waited for it can go.
        self.clone().flush(peer);
        match request {
            Request::Hello { roster: theirs, agents } => {
                if let Err(e) = self.merge(&theirs) {
                    tracing::warn!(error = format!("{e:#}"), "couldn't save the roster");
                }
                self.saw(peer, Some(agents));
                Response::Hello { roster: self.roster().unwrap_or(roster), agents: self.offered() }
            }
            Request::Deliver { envelope } => self.take(peer, envelope).await,
            Request::Join { .. } => unreachable!("handled above"),
        }
    }

    /// A letter for one of this computer's agents: checked, then handed to the app or knocked
    /// into the agent's harness.
    pub(super) async fn take(self: &Arc<Self>, peer: EndpointId, envelope: Envelope) -> Response {
        if let Err(e) = wire::check(&envelope) {
            return refuse(e.to_string());
        }
        // Without a network, only this computer's own agents write here.
        let roster = locked(&self.roster).clone();
        let name = |id: &EndpointId| roster.as_ref().map_or_else(|| "local".into(), |r| r.name(id));
        let reach = locked(&self.agents).iter().find(|(a, _)| a.id == envelope.to).map(|(_, r)| r.clone());
        let Some(reach) = reach else {
            return refuse(format!("{} has no agent called {}", name(&self.me()), envelope.to));
        };
        let key = format!("{peer}:{}", envelope.id);
        if locked(&self.taken).contains(&key) {
            // The sender didn't hear back the first time; it was already delivered.
            return Response::Stored;
        }
        let pair = format!("{peer}/{}>{}", envelope.from, envelope.to);
        let meta = envelope.meta.as_ref().map(ToString::to_string).unwrap_or_default();
        // Two answers to two different letters may well say the same thing ("Done.").
        let said = digest(&[&envelope.body, &meta, envelope.reply_to.as_deref().unwrap_or_default()]);
        if let Err(reason) = locked(&self.guard).admit(pair.clone(), envelope.hops, said, Instant::now()) {
            return refuse(reason);
        }
        match self.hand_over(&reach, envelope.open(name(&peer))).await {
            Ok(()) => {
                let mut taken = locked(&self.taken);
                if taken.len() >= TAKEN_MAX {
                    taken.pop_front();
                }
                taken.push_back(key);
                Response::Stored
            }
            Err(e) => {
                // Not delivered, so the same words may come again.
                locked(&self.guard).forget(&pair);
                refuse(format!("{e:#}"))
            }
        }
    }

    async fn hand_over(&self, reach: &Reach, message: Message) -> Result<()> {
        if *reach == Reach::App {
            return self.inbox.try_send(message).map_err(|_| anyhow!("too many unread messages; try again later"));
        }
        if !message.wake {
            locked(&self.held).keep(message);
            return Ok(());
        }
        let batch = locked(&self.held).with(message);
        let reply_command = locked(&self.reply_command).clone();
        let knocked = reach.knock(&batch, &reply_command).await;
        if knocked.is_err() {
            locked(&self.held).put_back(batch);
        }
        knocked
    }

    fn admit(self: &Arc<Self>, peer: EndpointId, network: &EndpointId, secret: &str, name: &str) -> Result<Response> {
        let mut guard = locked(&self.roster);
        let roster = guard.as_mut().ok_or_else(|| anyhow!("this computer isn't in a network"))?;
        if roster.network != *network {
            bail!("this invite is for another network");
        }
        if roster.is_removed(&peer) {
            bail!("this computer was removed from the network; reset its key to join again");
        }
        let name = roster.free_name(&check_name(name)?, &peer);
        // Single use: taken only once everything else checks out.
        let valid = locked(&self.invites).remove(secret).is_some_and(|until| until > Instant::now());
        if !valid {
            bail!("this invite expired or was already used");
        }
        let key = self.endpoint.secret_key();
        roster
            .sign(key, wire::now_ms(), Change::Admit { id: peer, name })
            .map_err(|_| anyhow!("this computer can no longer invite: only the owner and admins can"))?;
        let welcome = Response::Welcome { roster: roster.clone() };
        roster.save(&self.dir)?;
        drop(guard);
        self.saw(peer, Some(Vec::new()));
        tracing::info!(peer = %peer.fmt_short(), "a computer joined the network");
        self.clone().hello_all();
        Ok(welcome)
    }
}
