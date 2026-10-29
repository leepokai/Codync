//! Token estimates for the memory caps.
//!
//! Caps count tokens, not characters, so Chinese isn't penalised: 2,000 CJK characters are 2–3k tokens, 2,000
//! English characters about 500. The estimate counts one token per CJK character and one per four others.

/// The most the shared "yours" layer's `MEMORY.md` may hold, in estimated tokens.
pub const YOURS_CAP: usize = 400;
/// The most one bot's own `MEMORY.md` may hold, in estimated tokens.
pub const BOT_CAP: usize = 1_200;
/// The most one line of `MEMORY.md` may hold, in estimated tokens. Memories are short facts, not documents.
pub const LINE_CAP: usize = 150;

/// Estimated tokens in `text`.
pub fn estimate(text: &str) -> usize {
    let (cjk, other) =
        text.chars().fold((0, 0), |(cjk, other), ch| if is_cjk(ch) { (cjk + 1, other) } else { (cjk, other + 1) });
    cjk + usize::div_ceil(other, 4)
}

fn is_cjk(ch: char) -> bool {
    matches!(u32::from(ch),
        0x1100..=0x11FF // Hangul Jamo
        | 0x2E80..=0x2FDF // radicals, Kangxi
        | 0x3000..=0x31FF // CJK punctuation, kana, Bopomofo, Hangul compatibility
        | 0x3400..=0x4DBF // extension A
        | 0x4E00..=0x9FFF // unified ideographs
        | 0xAC00..=0xD7AF // Hangul syllables
        | 0xF900..=0xFAFF // compatibility ideographs
        | 0xFF00..=0xFFEF // fullwidth forms
        | 0x2_0000..=0x3_FFFF) // extensions B and later
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_cjk_per_character_and_latin_per_four() {
        assert_eq!(estimate(""), 0);
        assert_eq!(estimate("abcd"), 1);
        assert_eq!(estimate("abcde"), 2);
        assert_eq!(estimate("台北時間"), 4);
        assert_eq!(estimate("住在台北 ok"), 5);
        assert_eq!(estimate("한국어"), 3);
    }
}
