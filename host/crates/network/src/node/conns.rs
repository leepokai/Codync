//! Connections to the other computers: one kept open per computer, whichever side opened it, and
//! used for every call. A computer behind a firewall that lets in only what it started (Windows by
//! default) stays reachable over the connection it opened.

use super::{Inner, inbound, locked};
use crate::roster::Roster;
use crate::wire::{self, ALPN, Request, Response};
use anyhow::Result;
use iroh::endpoint::Connection;
use iroh::{Endpoint, EndpointAddr, EndpointId};
use std::sync::Arc;
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Long enough for the other computer to knock on its agent's harness.
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

impl Inner {
    /// Asks `id` over the open connection, or over a new one when there is none or the open one
    /// stopped answering (the other computer restarted or moved).
    pub(super) async fn call(self: &Arc<Self>, id: EndpointId, request: &Request) -> Result<Response> {
        if let Some(conn) = self.open(id) {
            match ask(&conn, request).await {
                Ok(answer) => return Ok(self.answered(&conn, request, answer)),
                Err(e) => {
                    tracing::debug!(peer = %id.fmt_short(), error = format!("{e:#}"), "the open connection stopped answering");
                    self.forget(&conn);
                }
            }
        }
        let conn = self.connect(id).await?;
        let answer = ask(&conn, request).await?;
        Ok(self.answered(&conn, request, answer))
    }

    /// A computer that refuses a hello doesn't count this one in (yet), so it didn't keep the
    /// connection either: dropped, so the next retry dials again once it may.
    fn answered(&self, conn: &Connection, request: &Request, answer: Response) -> Response {
        if matches!((request, &answer), (Request::Hello { .. }, Response::Refused { .. })) {
            self.forget(conn);
        }
        answer
    }

    /// Keeps a connection another member opened, for this computer's calls to it: it just proved
    /// that computer is there, and its firewall may let in nothing else. Anyone else's is only
    /// answered (a computer joining, or one removed that learns so).
    pub(super) fn accepted(&self, conn: &Connection) {
        let member = locked(&self.roster).as_ref().is_some_and(|r| r.is_member(&conn.remote_id()));
        if member {
            locked(&self.conns).insert(conn.remote_id(), conn.clone());
        }
    }

    /// Closes the kept connections to computers that are no longer members.
    pub(super) fn close_outsiders(&self, roster: &Roster) {
        locked(&self.conns).retain(|id, conn| {
            let member = roster.is_member(id);
            if !member {
                conn.close(0u32.into(), b"removed");
            }
            member
        });
        // A dial under way keeps its own lock.
        locked(&self.dialing).retain(|id, _| roster.is_member(id));
    }

    /// Keeps `conn` when none is open to its computer: one that wasn't a member yet when it
    /// connected may be by its next request.
    pub(super) fn adopt(&self, conn: &Connection) {
        if self.open(conn.remote_id()).is_none() {
            self.accepted(conn);
        }
    }

    /// The other members this computer has no open connection to.
    pub(super) fn unconnected(&self) -> Vec<EndpointId> {
        self.others().into_iter().filter(|id| self.open(*id).is_none()).collect()
    }

    fn open(&self, id: EndpointId) -> Option<Connection> {
        locked(&self.conns).get(&id).filter(|c| c.close_reason().is_none()).cloned()
    }

    fn forget(&self, conn: &Connection) {
        let id = conn.remote_id();
        let mut conns = locked(&self.conns);
        if conns.get(&id).is_some_and(|c| c.stable_id() == conn.stable_id()) {
            conns.remove(&id);
        }
        drop(conns);
        conn.close(0u32.into(), b"stale");
    }

    /// Opens a connection to `id`, one at a time per computer so two calls don't open two.
    async fn connect(self: &Arc<Self>, id: EndpointId) -> Result<Connection> {
        let dialing = locked(&self.dialing).entry(id).or_default().clone();
        let _one = dialing.lock().await;
        if let Some(conn) = self.open(id) {
            return Ok(conn);
        }
        let addr = locked(&self.roster)
            .as_ref()
            .and_then(|r| r.members().get(&id).map(|m| m.addr.clone()))
            .unwrap_or_else(|| EndpointAddr::new(id));
        let conn = dial(&self.endpoint, addr).await?;
        locked(&self.conns).insert(id, conn.clone());
        // The other computer asks over it too.
        tokio::spawn(inbound::take_requests(Arc::downgrade(self), conn.clone()));
        Ok(conn)
    }
}

impl Drop for Inner {
    // Ends the tasks answering on the kept connections, also for a node dropped without a shutdown.
    fn drop(&mut self) {
        for conn in locked(&self.conns).values() {
            conn.close(0u32.into(), b"gone");
        }
    }
}

/// Opens a connection to `addr`, within [`CONNECT_TIMEOUT`].
pub(super) async fn dial(endpoint: &Endpoint, addr: EndpointAddr) -> Result<Connection> {
    Ok(tokio::time::timeout(CONNECT_TIMEOUT, endpoint.connect(addr, ALPN)).await??)
}

/// One request and its answer, within [`CALL_TIMEOUT`].
pub(super) async fn ask(conn: &Connection, request: &Request) -> Result<Response> {
    tokio::time::timeout(CALL_TIMEOUT, wire::call(conn, request)).await?
}

#[cfg(test)]
mod tests {
    use super::super::Node;
    use super::super::testing::{agent, until};
    use super::*;
    use crate::types::{Connector, Letter, Sent};

    #[tokio::test]
    async fn a_computer_writes_back_over_the_connection_the_other_one_opened() {
        let root = std::env::temp_dir().join(format!("codync-network-conns-{}", wire::random_hex(8)));
        let (desk, _) = Node::start(&root.join("desk"), &Connector::Private).await.expect("desk starts");
        desk.create("desk").expect("a network");
        desk.set_agents([agent("lead")]);
        let (laptop, mut inbox) = Node::start(&root.join("laptop"), &Connector::Private).await.expect("laptop starts");
        laptop.set_agents([agent("coder")]);
        laptop.join(&desk.invite().expect("an invite"), "laptop").await.expect("joined");
        desk.peers().await.expect("the desk hears the laptop");

        // Both forget every connection (two may be open when both dialed at once), as if the
        // laptop's firewall let nothing in from the desk.
        for node in [&desk, &laptop] {
            for (_, conn) in locked(&node.inner.conns).drain() {
                conn.close(0u32.into(), b"test");
            }
        }
        locked(&desk.inner.dialing).clear();

        // The laptop's retry finds no open connection and opens one; the desk writes back over it.
        laptop.inner.retry().await;
        assert!(desk.inner.open(laptop.id()).is_some(), "the desk keeps the laptop's connection");
        let coder = desk.resolve("coder@laptop").expect("the laptop's agent");
        let sent = desk.send(Letter::new("lead", coder, "hello")).await.expect("sent");
        assert_eq!(sent, Sent::Delivered);
        assert_eq!(inbox.recv().await.expect("a letter").body, "hello");
        assert!(!locked(&desk.inner.dialing).contains_key(&laptop.id()), "the desk never dialed the laptop");

        desk.shutdown().await;
        laptop.shutdown().await;
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn a_computer_refused_by_another_dials_it_again_next_time() {
        let root = std::env::temp_dir().join(format!("codync-network-refused-{}", wire::random_hex(8)));
        let (desk, _) = Node::start(&root.join("desk"), &Connector::Private).await.expect("desk starts");
        desk.create("desk").expect("a network");
        let (laptop, _) = Node::start(&root.join("laptop"), &Connector::Private).await.expect("laptop starts");
        laptop.join(&desk.invite().expect("an invite"), "laptop").await.expect("joined");
        desk.peers().await.expect("the desk hears the laptop");

        // The desk no longer counts the laptop in (here: removed; or it hasn't heard of it yet).
        desk.remove("laptop", false).expect("removed");
        until("the laptop sees the desk close its connection", || laptop.inner.open(desk.id()).is_none()).await;
        assert!(laptop.inner.hello(desk.id()).await.is_err(), "the desk refuses the laptop");
        assert!(laptop.inner.open(desk.id()).is_none(), "the refused connection isn't kept for the next try");

        desk.shutdown().await;
        laptop.shutdown().await;
        let _ = std::fs::remove_dir_all(root);
    }
}
