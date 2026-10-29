//! Merge adjacent Live fragments for display without losing their original text or timing.

use super::super::{CallFragment, CallLine, CallState, Speaker};

const GAP_MS: i64 = 1_500;
const LINE_BYTES: usize = 2_000;
const MAX_FRAGMENTS: usize = 128;

/// Fragments may cross storage-line limits, but never a speaker, pause or session boundary.
pub(in crate::chat::calls) fn continues(last: &CallLine, speaker: Speaker, start_ms: i64, epoch: i64) -> bool {
    last.from == speaker
        && last.start_ms.is_some_and(|first| last.at == Some(epoch.saturating_add(first)) && start_ms >= first)
        && last.end_ms.is_some_and(|end| start_ms <= end.saturating_add(GAP_MS))
}

pub(super) fn append(s: &mut CallState, speaker: Speaker, text: &str, start_ms: i64, end_ms: i64, epoch: i64) {
    let mut rest = text;
    while !rest.is_empty() {
        let mut end = rest.len().min(LINE_BYTES);
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let (piece, tail) = rest.split_at(end);
        let fragment = CallFragment { delta: piece.to_owned(), start_ms, end_ms };
        let unheard = s.lines.len() > s.handoff;
        if let Some(last) = s.lines.last_mut().filter(|last| {
            unheard
                && continues(last, speaker, start_ms, epoch)
                && last.text.len() + piece.len() <= LINE_BYTES
                && last.fragments.len() < MAX_FRAGMENTS
        }) {
            last.text.push_str(piece);
            last.end_ms = Some(last.end_ms.unwrap_or(end_ms).max(end_ms));
            last.fragments.push(fragment);
        } else {
            super::super::append(
                s,
                vec![CallLine {
                    from: speaker,
                    text: piece.to_owned(),
                    at: Some(epoch.saturating_add(start_ms)),
                    id: Some(uuid::Uuid::new_v4().to_string()),
                    start_ms: Some(start_ms),
                    end_ms: Some(end_ms),
                    fragments: vec![fragment],
                }],
            );
        }
        rest = tail;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_fragments_form_one_line_and_keep_original_deltas() {
        let mut s = CallState::new("b");
        append(&mut s, Speaker::User, "Open app", 10, 20, 100);
        append(&mut s, Speaker::User, ".ts", 20, 30, 100);
        assert_eq!(s.lines.len(), 1);
        assert_eq!(s.lines[0].text, "Open app.ts");
        assert_eq!(s.lines[0].fragments.iter().map(|f| f.delta.as_str()).collect::<Vec<_>>(), ["Open app", ".ts"]);
        assert_eq!((s.lines[0].start_ms, s.lines[0].end_ms), (Some(10), Some(30)));
        // A handoff must never skip words appended afterwards.
        s.handoff = s.lines.len();
        append(&mut s, Speaker::User, " please", 30, 40, 100);
        assert_eq!(s.lines.len(), 2);
        assert_eq!(super::super::super::speech::user_words(&s.lines[s.handoff..]), " please");
    }

    #[test]
    fn pauses_speakers_reconnections_and_long_text_start_new_lines() {
        let mut s = CallState::new("b");
        append(&mut s, Speaker::User, "One", 0, 10, 100);
        append(&mut s, Speaker::User, "Two", 2000, 2010, 100);
        append(&mut s, Speaker::Voice, "Yes", 2010, 2020, 100);
        append(&mut s, Speaker::Voice, "Again", 2030, 2040, 200);
        assert_eq!(s.lines.len(), 4);
        let long = "你好👋".repeat(700);
        append(&mut s, Speaker::User, &long, 3000, 4000, 200);
        assert_eq!(s.lines[4..].iter().map(|l| l.text.as_str()).collect::<String>(), long);
        assert!(s.lines[4..].iter().all(|line| line.text.len() <= LINE_BYTES));
    }

    #[test]
    fn bot_handoff_preserves_turn_boundaries_and_split_words() {
        use super::super::super::speech::user_words;
        let mut s = CallState::new("b");
        append(&mut s, Speaker::User, "Open foo.ts", 0, 10, 100);
        append(&mut s, Speaker::Voice, "What next?", 10, 20, 100);
        append(&mut s, Speaker::User, "Run tests", 20, 30, 100);
        assert_eq!(user_words(&s.lines), "Open foo.ts Run tests");

        append(&mut s, Speaker::User, "Then lint", 3000, 3010, 100);
        assert_eq!(user_words(&s.lines), "Open foo.ts Run tests Then lint");
        append(&mut s, Speaker::User, "Again", 0, 10, 5000);
        assert_eq!(user_words(&s.lines), "Open foo.ts Run tests Then lint Again");

        // A single utterance split at the byte limit must retain exact CJK/filename spelling.
        let long = "你好app.ts".repeat(500);
        let mut split = CallState::new("b");
        append(&mut split, Speaker::User, &long, 0, 1000, 100);
        assert!(split.lines.len() > 1);
        assert_eq!(user_words(&split.lines), long);
    }
}
