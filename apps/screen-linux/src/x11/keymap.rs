//! The X server's core keyboard map: which keycode (and shift level) types a keysym, and spare
//! keycodes bound to keysyms the map lacks (any Unicode text, the xdotool way).

use std::time::{Duration, Instant};

/// How long a spare key keeps its binding after its last press. Apps read a key's binding when
/// they handle the press, which a busy one (xterm while the screen is encoded) does late: a
/// binding changed sooner types the next character instead, or nothing.
pub(super) const HOLD: Duration = Duration::from_secs(1);

/// How a keysym gets typed.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Stroke {
    /// A key the map has; `shift` for its second level.
    Mapped { code: u8, shift: bool },
    /// A spare key, bound to the keysym first (`syms` is its new map row), not before `ready`:
    /// when every spare is bound, the one pressed longest ago, once apps have read it.
    Spare {
        code: u8,
        syms: Vec<u32>,
        ready: Instant,
    },
}

/// Spare keys bound to a keysym, least recently pressed first, with when each was pressed. Kept
/// across events, so a binding stays until apps have read it.
#[derive(Default)]
pub(super) struct Bound(Vec<(u8, Instant)>);

impl Bound {
    fn holds(&self, code: u8) -> bool {
        self.0.iter().any(|&(c, _)| c == code)
    }

    fn pressed(&mut self, code: u8, at: Instant) {
        self.0.retain(|&(c, _)| c != code);
        self.0.push((code, at));
    }

    /// The keys last pressed `HOLD` before `now` or earlier, to clear.
    pub(super) fn expired(&mut self, now: Instant) -> Vec<u8> {
        let (old, live): (Vec<_>, _) = std::mem::take(&mut self.0)
            .into_iter()
            .partition(|&(_, at)| at + HOLD <= now);
        self.0 = live;
        old.into_iter().map(|(code, _)| code).collect()
    }
}

pub(super) struct Keymap {
    min: u8,
    per: usize,
    /// `per` keysyms for each keycode from `min` on.
    syms: Vec<u32>,
    /// Keycodes with no keysym at all.
    spares: Vec<u8>,
}

impl Keymap {
    pub(super) fn new(min: u8, per: u8, syms: Vec<u32>) -> Self {
        let per = usize::from(per.max(1));
        let mut map = Self {
            min,
            per,
            syms,
            spares: vec![],
        };
        map.spares = map
            .syms
            .chunks(per)
            .enumerate()
            .filter(|(_, row)| row.iter().all(|&s| s == 0))
            .filter_map(|(i, _)| map.code(i))
            .collect();
        map
    }

    pub(super) fn per(&self) -> u8 {
        u8::try_from(self.per).unwrap_or(u8::MAX)
    }

    fn code(&self, row: usize) -> Option<u8> {
        u8::try_from(usize::from(self.min) + row).ok()
    }

    /// The keycode for `sym` on its first or second (shifted) level, unshifted preferred.
    pub(super) fn find(&self, sym: u32) -> Option<(u8, bool)> {
        if sym == 0 {
            return None;
        }
        [false, true].into_iter().find_map(|shift| {
            let level = usize::from(shift);
            let row = self
                .syms
                .chunks(self.per)
                .position(|row| row.get(level) == Some(&sym))?;
            Some((self.code(row)?, shift))
        })
    }

    /// The key that types `sym` at `now`, binding a spare one when the map has none.
    pub(super) fn stroke(&mut self, sym: u32, bound: &mut Bound, now: Instant) -> Option<Stroke> {
        if let Some((code, shift)) = self.find(sym) {
            if bound.holds(code) {
                bound.pressed(code, now);
            }
            return Some(Stroke::Mapped { code, shift });
        }
        if sym == 0 {
            return None;
        }
        let free = self.spares.iter().find(|&&c| !bound.holds(c));
        let (code, ready) = free.map(|&code| (code, now)).or_else(|| {
            let &(code, at) = bound.0.first()?;
            Some((code, now.max(at + HOLD)))
        })?;
        bound.pressed(code, ready);
        let mut syms = vec![0; self.per];
        syms[0] = sym;
        if let Some(shifted) = syms.get_mut(1) {
            *shifted = sym;
        }
        let start = usize::from(code - self.min) * self.per;
        self.syms[start..start + self.per].copy_from_slice(&syms);
        Some(Stroke::Spare { code, syms, ready })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const RETURN: u32 = 0xff0d;

    fn map() -> Keymap {
        // Keycodes 8: a/A, 9: spare, 10: Return, 11: spare.
        Keymap::new(8, 2, vec![0x61, 0x41, 0, 0, RETURN, 0, 0, 0])
    }

    #[test]
    fn mapped_keys_and_their_shift_level() {
        let map = map();
        assert_eq!(map.find(0x61), Some((8, false)));
        assert_eq!(map.find(0x41), Some((8, true)));
        assert_eq!(map.find(RETURN), Some((10, false)));
        assert_eq!(map.find(0), None);
    }

    #[test]
    fn spare_keys_keep_their_binding_until_apps_have_read_it() {
        let (e, u, s) = (0xe9, 0x0100_4e2d, 0xdf);
        let mut bound = Bound::default();
        let t0 = Instant::now();
        let mut map = map();
        let spare = |code, sym, ready| {
            Some(Stroke::Spare {
                code,
                syms: vec![sym, sym],
                ready,
            })
        };
        assert_eq!(map.stroke(e, &mut bound, t0), spare(9, e, t0));
        // Typed again while bound: no new binding.
        let t1 = t0 + Duration::from_millis(10);
        assert_eq!(
            map.stroke(e, &mut bound, t1),
            Some(Stroke::Mapped {
                code: 9,
                shift: false
            })
        );
        // The next event reads the map again: its bindings stay, the other spare is free.
        let mut map = Keymap::new(8, 2, map.syms.clone());
        let t2 = t0 + Duration::from_millis(20);
        assert_eq!(map.stroke(u, &mut bound, t2), spare(11, u, t2));
        // Every spare is bound: `é`'s key, pressed longest ago, once apps have read it.
        assert_eq!(map.stroke(s, &mut bound, t2), spare(9, s, t1 + HOLD));
        assert_eq!(
            map.stroke(0x61, &mut bound, t2),
            Some(Stroke::Mapped {
                code: 8,
                shift: false
            })
        );
        assert_eq!(bound.expired(t2 + HOLD), vec![11]);
        assert_eq!(bound.expired(t1 + HOLD + HOLD), vec![9]);
        assert_eq!(bound.expired(t1 + HOLD * 3), [] as [u8; 0]);
    }

    #[test]
    fn a_full_map_cannot_type_what_it_lacks() {
        let mut map = Keymap::new(8, 1, vec![0x61]);
        assert_eq!(
            map.stroke(0xe9, &mut Bound::default(), Instant::now()),
            None
        );
    }
}
