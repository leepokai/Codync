//! What this computer answers: joins, hellos and letters for its agents.

use super::{Inner, check_name, locked, remember};
use crate::guard::digest;
use crate::reach::Reach;
use crate::roster::Change;
use crate::rules;
use crate::types::Message;
use crate::wire::{self, Envelope, Request, Response};
use anyhow::{Result, anyhow, bail};
use iroh::EndpointId;
use iroh::endpoint::{Connection, RecvStream, SendStream};
use iroh::protocol::{AcceptError, ProtocolHandler};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::Ordering;
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
        if !self.ready.load(Ordering::Acquire) {
            return Response::Later { reason: "this computer is still starting".into() };
        }
        let key = format!("{peer}:{}", envelope.id);
        if locked(&self.taken).contains(&key) {
            // The sender didn't hear back the first time; it was already delivered.
            return Response::Stored;
        }
        // Without a network, only this computer's own agents write here.
        let roster = locked(&self.roster).clone();
        let name = |id: &EndpointId| roster.as_ref().map_or_else(|| "local".into(), |r| r.name(id));
        // This computer's rules, for letters from the others that don't answer one of its own.
        let answers = self.answers(peer, &envelope);
        if peer != self.me()
            && !answers
            && !rules::allows(&locked(&self.rules), &name(&peer), &envelope.from, &envelope.to)
        {
            return refuse(format!("{} doesn't take messages from {}@{}", envelope.to, envelope.from, name(&peer)));
        }
        let reach = locked(&self.agents).iter().find(|(a, _)| a.id == envelope.to).map(|(_, r)| r.clone());
        let reach = match reach {
            Some(reach) => reach,
            // An answer to an agent that has since been renamed, gone or kept off the network: the
            // app knows its request by id and still takes it. Only a real answer: one this
            // computer asked for.
            None if envelope.reply_to.is_some() && answers => Reach::App,
            None => return refuse(format!("{} has no agent called {}", name(&self.me()), envelope.to)),
        };
        let pair = format!("{peer}/{}>{}", envelope.from, envelope.to);
        let meta = envelope.meta.as_ref().map(ToString::to_string).unwrap_or_default();
        // Two answers to two different letters may well say the same thing ("Done.").
        let said = digest(&[&envelope.body, &meta, envelope.reply_to.as_deref().unwrap_or_default()]);
        if let Err(reason) = locked(&self.guard).admit(pair.clone(), envelope.hops, said, Instant::now()) {
            return refuse(reason);
        }
        // An answer that says how the request ended (`meta`) is the last one past the rules.
        let ended = envelope.reply_to.clone().filter(|_| answers && envelope.meta.is_some());
        // A recipient can reply as soon as delivery wakes it, before the handoff returns.
        // Keep this bounded metadata even on failure: a harness may already have seen the letter.
        self.heard(&envelope.id, envelope.hops);
        let handed_over = self.hand_over(&reach, envelope.open(name(&peer))).await;
        match handed_over {
            Ok(()) => {
                remember(&mut locked(&self.taken), key, TAKEN_MAX);
                if let Some(request) = ended {
                    locked(&self.asked).retain(|(sent, to, _)| *sent != request || *to != peer);
                }
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
        self.save(roster)?;
        drop(guard);
        self.saw(peer, Some(Vec::new()));
        tracing::info!(peer = %peer.fmt_short(), "a computer joined the network");
        self.clone().hello_all();
        Ok(welcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Node;
    use crate::types::{Connector, Letter, Sent};
    use crate::wire::{Agent, AgentKind};
    use std::sync::Mutex;
    use std::task::{Context, Wake, Waker};

    struct ObserveHops {
        inner: Arc<Inner>,
        letter: String,
        observed: Mutex<Option<u8>>,
    }

    impl Wake for ObserveHops {
        fn wake(self: Arc<Self>) {
            *locked(&self.observed) = self.inner.hops_of(&self.letter);
        }
    }

    #[tokio::test]
    async fn reply_hops_are_available_when_delivery_wakes_the_recipient() {
        let dir = std::env::temp_dir().join(format!("codync-network-hops-{}", wire::random_hex(8)));
        let started = Node::start(&dir, &Connector::Private).await;
        let (node, mut inbox) = started.expect("start an isolated node");
        node.set_agents(["sender", "recipient"].map(|id| {
            (Agent { id: id.into(), name: id.into(), about: String::new(), kind: AgentKind::Codync }, Reach::App)
        }));
        let recipient = node.resolve("recipient").expect("local recipient");
        let letter = Letter::new("sender", recipient, "reply immediately").hops(3);
        let observer =
            Arc::new(ObserveHops { inner: node.inner.clone(), letter: letter.id.clone(), observed: Mutex::new(None) });
        let waker = Waker::from(observer.clone());
        let mut context = Context::from_waker(&waker);
        assert!(inbox.poll_recv(&mut context).is_pending());

        let sent = node.send(letter).await;
        assert_eq!(sent.expect("deliver locally"), Sent::Delivered);
        let delivered_hops = *locked(&observer.observed);
        node.shutdown().await;
        std::fs::remove_dir_all(dir).expect("remove isolated state");
        assert_eq!(delivered_hops, Some(3), "an immediately woken reader must retain the reply chain");
    }
}
