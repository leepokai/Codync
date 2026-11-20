//! Hellos: swapping rosters and agent lists with the other members, so changes and offers spread.

use super::{Inner, locked};
use crate::roster::Roster;
use crate::wire::{Agent, Request, Response};
use anyhow::{Result, bail};
use iroh::EndpointId;
use std::sync::Arc;
use tokio::task::JoinSet;

impl Inner {
    /// Swaps rosters and agent lists with one member.
    pub(super) async fn hello(self: &Arc<Self>, id: EndpointId) -> Result<Vec<Agent>> {
        let request = Request::Hello { roster: self.roster()?, agents: self.offered() };
        let answer = self.call(id, &request).await;
        let agents = match answer {
            Ok(Response::Hello { roster, agents }) => {
                self.merge(&roster)?;
                self.clone().flush(id);
                agents
            }
            Ok(Response::Refused { reason }) => {
                self.saw(id, None);
                bail!("{reason}")
            }
            Ok(_) => bail!("unexpected answer"),
            Err(e) => {
                self.saw(id, None);
                return Err(e);
            }
        };
        self.saw(id, Some(agents.clone()));
        Ok(agents)
    }

    /// Says hello to every other member in the background, so a change reaches them.
    pub(super) fn hello_all(self: Arc<Self>) {
        tokio::spawn(async move { self.hello_each(self.others()).await });
    }

    /// Says hello to each of `ids` at once and waits for every answer.
    pub(super) async fn hello_each(self: &Arc<Self>, ids: impl IntoIterator<Item = EndpointId>) {
        let mut asks = JoinSet::new();
        for id in ids {
            let inner = self.clone();
            asks.spawn(async move {
                if let Err(e) = inner.hello(id).await {
                    tracing::debug!(peer = %id.fmt_short(), error = format!("{e:#}"), "hello didn't get through");
                }
            });
        }
        while asks.join_next().await.is_some() {}
    }

    /// Every member but this computer.
    pub(super) fn others(&self) -> Vec<EndpointId> {
        let me = self.me();
        locked(&self.roster)
            .as_ref()
            .map(|r| r.members().keys().copied().filter(|id| *id != me).collect())
            .unwrap_or_default()
    }

    pub(super) fn merge(&self, theirs: &Roster) -> Result<()> {
        let mut guard = locked(&self.roster);
        let Some(mine) = guard.as_mut() else { return Ok(()) };
        if mine.merge(theirs) {
            self.save(mine)?;
        }
        Ok(())
    }
}
