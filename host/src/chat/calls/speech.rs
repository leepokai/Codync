//! Text as a voice says it.

use super::{CallLine, Speaker};

/// Text as speech can carry it: code blocks and links named, not read, markdown marks dropped, capped.
pub(super) fn spoken(text: &str, max: usize) -> String {
    let mut out = String::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            if !in_code {
                out.push_str(" (code) ");
            }
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        let words: Vec<String> = line
            .split_whitespace()
            .map(|w| {
                if w.starts_with("http://") || w.starts_with("https://") {
                    "(link)".to_owned()
                } else {
                    // Inline code and emphasis can span words ("`cargo test`."): their marks go wherever they are.
                    w.replace(['`', '*'], "").trim_matches(['#', '>', '_']).to_owned()
                }
            })
            .filter(|w| !w.is_empty())
            .collect();
        out.push_str(&words.join(" "));
        out.push(' ');
    }
    crate::agent::acp::truncate(out.split_whitespace().collect::<Vec<_>>().join(" ").as_str(), max)
}

/// The call's lines, newest last, as many of the newest as fit `cap` characters.
pub(super) fn call_so_far(lines: &[CallLine], cap: usize) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0;
    for line in lines.iter().rev() {
        let who = match line.from {
            Speaker::User => "User",
            Speaker::Voice => "You",
        };
        let text = format!("{who}: {}", spoken(&line.text, 400));
        used += text.chars().count() + 1;
        if used > cap {
            break;
        }
        kept.push(text);
    }
    kept.reverse();
    kept.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_names_code_and_links() {
        assert_eq!(
            spoken("**Done.** See https://x.dev\n```\nlet a = 1;\n```\n- `cargo test` passed", 200),
            "Done. See (link) (code) - cargo test passed"
        );
        assert!(spoken(&"word ".repeat(500), 50).chars().count() <= 51);
        assert_eq!(
            spoken("Run `cargo test --release`. **Done**, keep snake_case", 200),
            "Run cargo test --release. Done, keep snake_case"
        );
    }

    #[test]
    fn a_replaced_session_hears_the_newest_of_the_call_so_far() {
        let line = |from, text: &str| CallLine { from, text: text.into(), at: None, id: None };
        let lines =
            [line(Speaker::User, "first thing"), line(Speaker::Voice, "noted"), line(Speaker::User, "second thing")];
        assert_eq!(call_so_far(&lines, 1_000), "User: first thing\nYou: noted\nUser: second thing");
        assert_eq!(call_so_far(&lines, 20), "User: second thing", "the newest lines win when they don't all fit");
        assert_eq!(call_so_far(&[], 1_000), "");
    }
}
