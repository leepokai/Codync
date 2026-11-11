//! A bounded caller may disappear while the actor awaits session or model setup.

use super::Actor;
use anyhow::anyhow;

impl Actor {
    pub(super) fn finish_expired_request(&mut self) -> bool {
        if !self.active_request.as_ref().is_some_and(crate::chat::team::BotRequest::reply_closed) {
            return false;
        }
        self.stop_requested = true;
        // Nothing reached session/prompt. An error with stop_requested settles as cancellation
        // without acknowledging profile delivery or turning the recipient into a broken bot.
        self.finish_turn(&Err(anyhow!("bot request cancelled before execution")));
        true
    }
}
