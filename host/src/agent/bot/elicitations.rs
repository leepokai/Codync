//! The agent's own questions (`elicitation/create`) wait on choice cards as `request_decision`'s do, but with no time
//! limit and never answered by the bot's approval setting: the user's answer goes back as `accept`, and a card that
//! stops waiting unanswered (the turn ends or is stopped, the actor goes away) as `cancel`.

use super::Actor;
use crate::agent::elicitation::{self, Declined, Form};
use crate::chat::decision::Asker;
use codync_agent::acp::Request;
use serde_json::Value;
use tokio::sync::oneshot;

impl Actor {
    /// An `elicitation/create` while a turn runs: a card the user answers, or declined when no card can ask it.
    pub(super) async fn on_elicitation(&mut self, request: Request) {
        self.expire_abandoned_decisions();
        if self.conn.is_none() {
            return;
        }
        let form =
            match Form::parse(request.params()).and_then(|form| self.shows(&form, request.params()).map(|()| form)) {
                Ok(form) => form,
                Err(why) => return self.decline_elicitation(request, why).await,
            };
        let (reply, answered) = oneshot::channel();
        self.post_decision(form.card(), None, Asker::Agent, reply);
        let hub = self.hub.clone();
        // The card's waiter gets the answer, or is dropped or told it expired when the card stops waiting.
        tokio::spawn(async move {
            let answer = answered.await.ok().and_then(Result::ok);
            let reply = form.reply(answer.as_ref());
            elicitation::report(&hub, &reply, None);
            if let Err(error) = request.respond(reply).await {
                tracing::warn!(error = format!("{error:#}"), "couldn't answer the agent's question");
            }
        });
    }

    /// Tells the agent no card asks its question.
    pub(super) async fn decline_elicitation(&self, request: Request, why: Declined) {
        tracing::info!(bot = %self.id(), reason = ?why, "declined the agent's question");
        let reply = elicitation::decline();
        elicitation::report(&self.hub, &reply, Some(why));
        if let Err(error) = request.respond(reply).await {
            tracing::warn!(error = format!("{error:#}"), "couldn't decline the agent's question");
        }
    }

    /// Whether the running turn shows `form` on a card: in its own session, where a card may ask, one at a time.
    fn shows(&self, form: &Form, params: &Value) -> Result<(), Declined> {
        if params["sessionId"].as_str().is_none_or(|sid| Some(sid) != self.turn_session.as_deref()) {
            return Err(Declined::Session);
        }
        // A thread's card takes presses only, so a question only typed words answer can't wait there.
        if !self.can_ask() || (form.free_text() && self.cfg.is_thread()) {
            return Err(Declined::Elsewhere);
        }
        if !self.asked.is_empty() {
            return Err(Declined::Busy);
        }
        Ok(())
    }
}
