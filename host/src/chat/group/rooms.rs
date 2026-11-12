//! Room turns per group: which one is current, which one runs, and the late answers waiting for one.
//!
//! A member's answer can land after the room turn that asked it stopped waiting (it was busy elsewhere, or ran
//! past [`MEMBER_TIMEOUT`]). Who it @-mentions gets a *late* room turn: right away when no room turn runs, or
//! once the running one ends. A message of the user's allows [`MAX_LATE_TURNS`] of them; Stop allows none
//! until the user writes again.

use super::MEMBER_TIMEOUT;
use crate::LockExt;
use crate::store::Lane;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

/// Late room turns per message of the user's, so members handing work back and forth late can't go on forever.
pub(super) const MAX_LATE_TURNS: u32 = 3;

/// A room turn to run.
pub(super) struct Turn {
    pub epoch: u64,
    /// A late turn: the `seq` of the oldest late answer it follows up (`None`: the user's message started it).
    pub late: Option<i64>,
}

#[derive(Default)]
struct Room {
    /// Moves with every room turn and with Stop: a turn whose epoch isn't current ends before its next speaker.
    epoch: u64,
    /// The epoch of the room turn running now.
    running: Option<u64>,
    /// The oldest late answer (its `seq`) no room turn has followed up yet.
    pending: Option<i64>,
    /// Late room turns since the user's last message.
    late_turns: u32,
}

impl Room {
    fn begin(&mut self, late: Option<i64>) -> Turn {
        self.epoch += 1;
        self.running = Some(self.epoch);
        Turn { epoch: self.epoch, late }
    }

    /// The late room turn for the pending late answers, while the allowance lasts.
    fn follow_up(&mut self) -> Option<Turn> {
        let late = self.pending.take()?;
        if self.late_turns >= MAX_LATE_TURNS {
            return None;
        }
        self.late_turns += 1;
        Some(self.begin(Some(late)))
    }
}

/// Room turns per group (keyed by its lane's chat).
pub struct Rooms {
    rooms: Mutex<HashMap<String, Room>>,
    /// How long the room waits on a member: [`MEMBER_TIMEOUT`].
    member_timeout: Mutex<Duration>,
}

impl Default for Rooms {
    fn default() -> Self {
        Self { rooms: Mutex::default(), member_timeout: Mutex::new(MEMBER_TIMEOUT) }
    }
}

impl Rooms {
    /// The user wrote in `lane`: its room turn, which ends the one running there. Marked running before it is
    /// spawned, so a late answer landing meanwhile waits for it instead of replacing it.
    pub(super) fn start(&self, lane: &Lane) -> Turn {
        let mut rooms = self.rooms.locked();
        let room = rooms.entry(lane.chat.clone()).or_default();
        room.pending = None;
        room.late_turns = 0;
        room.begin(None)
    }

    /// A member's answer that @-mentions someone landed at `seq` after its room turn stopped waiting: the late
    /// room turn to run now, or `None` when one runs (it follows the answer up as it ends) or none is allowed.
    pub(super) fn late(&self, lane: &Lane, seq: i64) -> Option<Turn> {
        let mut rooms = self.rooms.locked();
        let room = rooms.entry(lane.chat.clone()).or_default();
        room.pending = Some(room.pending.map_or(seq, |p| p.min(seq)));
        if room.running.is_some() {
            return None;
        }
        room.follow_up()
    }

    /// `epoch`'s room turn ended: the late room turn that follows up what landed meanwhile, if any.
    pub(super) fn ended(&self, lane: &Lane, epoch: u64) -> Option<Turn> {
        let mut rooms = self.rooms.locked();
        let room = rooms.get_mut(&lane.chat)?;
        if room.running != Some(epoch) {
            return None;
        }
        room.running = None;
        room.follow_up()
    }

    pub fn current(&self, lane: &Lane, epoch: u64) -> bool {
        self.rooms.locked().get(&lane.chat).is_some_and(|room| room.epoch == epoch)
    }

    /// Stop in `group`: ends its room turn, and late answers start none until the user writes again. `false`
    /// when no room turn ever ran there.
    pub(super) fn stop(&self, group: &str) -> bool {
        let mut rooms = self.rooms.locked();
        let Some(room) = rooms.get_mut(group) else { return false };
        room.epoch += 1;
        room.pending = None;
        room.late_turns = MAX_LATE_TURNS;
        true
    }

    pub(super) fn member_timeout(&self) -> Duration {
        *self.member_timeout.locked()
    }

    #[cfg(test)]
    pub(super) fn set_member_timeout(&self, timeout: Duration) {
        *self.member_timeout.locked() = timeout;
    }

    /// A room turn is running in `lane`'s group.
    #[cfg(test)]
    pub(super) fn running(&self, lane: &Lane) -> bool {
        self.rooms.locked().get(&lane.chat).is_some_and(|room| room.running.is_some())
    }

    /// A late answer in `lane`'s group waits for the running room turn to end.
    #[cfg(test)]
    pub(super) fn pending(&self, lane: &Lane) -> bool {
        self.rooms.locked().get(&lane.chat).is_some_and(|room| room.pending.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Late answers wait for the room turn that runs, start a few late turns per message of the user's at most,
    /// and none after Stop.
    #[test]
    fn late_room_turns_wait_their_turn_and_are_bounded() {
        let rooms = Rooms::default();
        let main = Lane::main("g");
        let first = rooms.start(&main);
        assert!(rooms.late(&main, 7).is_none(), "the user's turn runs from the start: a late answer waits for it");
        assert!(rooms.late(&main, 5).is_none());
        assert!(rooms.current(&main, first.epoch), "and doesn't end it");
        let follow_up = rooms.ended(&main, first.epoch).expect("its end follows the late answers up");
        assert_eq!(follow_up.late, Some(5), "from the oldest one");
        assert!(rooms.ended(&main, first.epoch).is_none(), "an old turn ending changes nothing");
        assert!(rooms.ended(&main, follow_up.epoch).is_none(), "nothing landed meanwhile");

        for _ in 1..MAX_LATE_TURNS {
            let late = rooms.late(&main, 9).expect("no turn runs: a late one starts");
            assert!(rooms.ended(&main, late.epoch).is_none());
        }
        assert!(rooms.late(&main, 9).is_none(), "members handing work back and forth late stop here");

        let next = rooms.start(&main);
        assert!(rooms.ended(&main, next.epoch).is_none());
        let late = rooms.late(&main, 12).expect("the user's next message allows more");
        assert!(rooms.ended(&main, late.epoch).is_none());
        assert!(rooms.stop("g"));
        assert!(rooms.late(&main, 13).is_none(), "none after Stop");
        assert!(!rooms.stop("other"), "no room turn ran there");
    }
}
