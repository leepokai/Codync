//! The X core keyboard mapping, read for each key event: which keycode types a keysym, and a
//! spare keycode for symbols the layout lacks.

pub const XK_SHIFT_L: u32 = 0xffe1;

/// `per` keysyms for each keycode from `min`.
pub struct Keymap {
    pub min: u8,
    pub per: u8,
    pub syms: Vec<u32>,
}

impl Keymap {
    fn rows(&self) -> impl Iterator<Item = (u8, &[u32])> {
        let per = usize::from(self.per).max(1);
        self.syms
            .chunks(per)
            .enumerate()
            .filter_map(|(i, row)| Some((self.min.checked_add(u8::try_from(i).ok()?)?, row)))
    }

    /// Keycode for a keysym and whether it needs Shift (column 1). Unshifted matches win.
    pub fn find(&self, sym: u32) -> Option<(u8, bool)> {
        if sym == 0 {
            return None;
        }
        for column in 0..2 {
            if let Some((code, _)) = self.rows().find(|(_, row)| row.get(column) == Some(&sym)) {
                return Some((code, column == 1));
            }
        }
        None
    }

    /// A keycode with no keysyms, free for a temporary binding.
    pub fn spare(&self) -> Option<u8> {
        self.rows()
            .filter(|(_, row)| row.iter().all(|&s| s == 0))
            .map(|(code, _)| code)
            .last()
    }

    /// One keycode's row with `sym` in every column, so Shift and the layout group don't matter.
    pub fn bound_row(&self, sym: u32) -> Vec<u32> {
        vec![sym; self.width()]
    }

    /// The row that clears a temporary binding.
    pub fn empty_row(&self) -> Vec<u32> {
        vec![0; self.width()]
    }

    fn width(&self) -> usize {
        usize::from(self.per).max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keymap_finds_shift_levels_and_a_spare_keycode() {
        // keycode 8: a/A, 9: 1/exclam, 10: Shift_L, 11: empty, 12: 2/at
        let map = Keymap {
            min: 8,
            per: 2,
            syms: vec![0x61, 0x41, 0x31, 0x21, XK_SHIFT_L, 0, 0, 0, 0x32, 0x40],
        };
        assert_eq!(map.find(0x61), Some((8, false)));
        assert_eq!(map.find(0x41), Some((8, true)));
        assert_eq!(map.find(0x21), Some((9, true)));
        assert_eq!(map.find(XK_SHIFT_L), Some((10, false)));
        assert_eq!(map.find(0xe9), None);
        assert_eq!(map.find(0), None);
        assert_eq!(map.spare(), Some(11));
        assert_eq!(map.bound_row(0xe9), vec![0xe9, 0xe9]);
        assert_eq!(map.empty_row(), vec![0, 0]);
        let wide = Keymap { per: 4, ..map };
        assert_eq!(wide.bound_row(0xe9), vec![0xe9; 4]);
        let full = Keymap {
            min: 8,
            per: 1,
            syms: vec![0x61, 0x62],
        };
        assert_eq!(full.find(0x62), Some((9, false)));
        assert_eq!(full.spare(), None);
    }
}
