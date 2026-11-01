//! Bare web addresses in message text (`https://…`, `http://…`, `www.…`), found by the same rule
//! as the desktop, iPhone and Android apps: the address runs to a space, quote, angle bracket,
//! backtick, star or CJK punctuation, without the sentence's closing punctuation or an unmatched
//! closing bracket. They are underlined, and a click on one opens it in the browser.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::md::width;

const PREFIXES: [&str; 3] = ["https://", "http://", "www."];
const TRAILING: &str = ".,:;!?'\"*_~";

/// The length in chars of the address starting at `chars[at]`, 0 for none. An address
/// doesn't start inside a word (`xhttps://…`).
pub fn url_len(chars: &[char], at: usize) -> usize {
    if at > 0 && (chars[at - 1].is_alphanumeric() || chars[at - 1] == '_') {
        return 0;
    }
    let rest = &chars[at..];
    let Some(prefix) = PREFIXES
        .iter()
        .find(|p| p.len() <= rest.len() && p.chars().zip(rest).all(|(a, b)| b.eq_ignore_ascii_case(&a)))
        .map(|p| p.len())
    else {
        return 0;
    };
    let mut end = rest.iter().position(|&c| stops(c)).unwrap_or(rest.len());
    // Bracket counts are kept while trimming, so a long run of `)` costs one pass.
    let count = |c: char| rest[..end].iter().filter(|&&x| x == c).count();
    let (mut parens, mut brackets) = ((count('('), count(')')), (count('['), count(']')));
    while end > 0 {
        let pair = match rest[end - 1] {
            ')' => Some(&mut parens),
            ']' => Some(&mut brackets),
            _ => None,
        };
        match pair {
            Some((open, close)) if *close > *open => *close -= 1,
            None if TRAILING.contains(rest[end - 1]) => {}
            Some(_) | None => break,
        }
        end -= 1;
    }
    if end > prefix && rest[prefix..end].iter().any(|c| c.is_alphanumeric()) { end } else { 0 }
}

fn stops(c: char) -> bool {
    c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`' | '*' | '\u{3000}'..='\u{303f}' | '\u{ff00}'..='\u{ffef}')
}

/// What an address opens: `www.` addresses over https, and the scheme in lowercase.
pub fn href(url: &str) -> String {
    if url.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("www.")) {
        return format!("https://{url}");
    }
    let (scheme, rest) = url.split_at(url.find(':').unwrap_or(0));
    format!("{}{rest}", scheme.to_ascii_lowercase())
}

/// Plain text (a message the user wrote) with its addresses underlined.
pub fn underline(text: &str, style: Style) -> Vec<Span<'static>> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let (mut from, mut i) = (0, 0);
    while i < chars.len() {
        let n = url_len(&chars, i);
        if n == 0 {
            i += 1;
            continue;
        }
        if from < i {
            out.push(Span::styled(chars[from..i].iter().collect::<String>(), style));
        }
        out.push(Span::styled(chars[i..i + n].iter().collect::<String>(), style.add_modifier(Modifier::UNDERLINED)));
        i += n;
        from = i;
    }
    if from < chars.len() {
        out.push(Span::styled(chars[from..].iter().collect::<String>(), style));
    }
    out
}

/// One underlined run of a line: where it is and its text.
struct Run {
    line: usize,
    x: usize,
    text: String,
    /// It ends the line's text at the wrap edge, so the address may go on in the next line.
    at_edge: bool,
    /// Only spaces or a quote bar come before it in its line.
    leads: bool,
}

/// Where the addresses are in wrapped lines `width` cells wide, as (line, x, width, what it
/// opens). An address cut at the wrap edge is one link across its lines.
pub fn hits(lines: &[Line<'_>], width: usize) -> Vec<(usize, u16, u16, String)> {
    let mut out = vec![];
    let mut chain: Vec<Run> = vec![];
    for run in lines.iter().enumerate().flat_map(|(i, l)| runs(i, l, width)) {
        // A run that starts with its own prefix is a new address, not the rest of the one above.
        let starts = url_len(&run.text.chars().collect::<Vec<_>>(), 0) > 0;
        let joins = !starts && chain.last().is_some_and(|last| last.at_edge && last.line + 1 == run.line && run.leads);
        if !joins {
            link(&mut out, std::mem::take(&mut chain));
        }
        chain.push(run);
    }
    link(&mut out, chain);
    out
}

fn link(out: &mut Vec<(usize, u16, u16, String)>, chain: Vec<Run>) {
    let chars: Vec<char> = chain.iter().flat_map(|r| r.text.chars()).collect();
    if chars.is_empty() || url_len(&chars, 0) != chars.len() {
        return;
    }
    let url = href(&chars.iter().collect::<String>());
    out.extend(chain.into_iter().map(|r| (r.line, cells(r.x), cells(width(&r.text)), url.clone())));
}

fn cells(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn runs(line: usize, l: &Line<'_>, wrap: usize) -> Vec<Run> {
    let end = width(l.to_string().trim_end());
    let mut out: Vec<Run> = vec![];
    let (mut x, mut leads) = (0, true);
    let mut open = false;
    for span in &l.spans {
        let underlined = span.style.add_modifier.contains(Modifier::UNDERLINED);
        match out.last_mut() {
            Some(run) if underlined && open => run.text.push_str(&span.content),
            _ if underlined => out.push(Run { line, x, text: span.content.to_string(), at_edge: false, leads }),
            _ => {}
        }
        open = underlined;
        leads &= span.content.chars().all(|c| c == ' ' || c == '▎');
        x += width(&span.content);
    }
    if let Some(run) = out.last_mut() {
        let run_end = run.x + width(&run.text);
        run.at_edge = run_end == end && run_end + 2 >= wrap;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn urls(text: &str) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut out = vec![];
        let mut i = 0;
        while i < chars.len() {
            match url_len(&chars, i) {
                0 => i += 1,
                n => {
                    out.push(href(&chars[i..i + n].iter().collect::<String>()));
                    i += n;
                }
            }
        }
        out
    }

    // The same cases as the desktop's, iPhone's and Android's autolink tests.
    #[test]
    fn finds_bare_addresses_and_leaves_the_sentence_around_them() {
        assert_eq!(urls("see https://codync.dev."), ["https://codync.dev"]);
        assert_eq!(urls("open www.example.com/a?b=1, then"), ["https://www.example.com/a?b=1"]);
        assert_eq!(urls("(at https://example.com/x)"), ["https://example.com/x"]);
        assert_eq!(
            urls("https://en.wikipedia.org/wiki/Rust_(language)"),
            ["https://en.wikipedia.org/wiki/Rust_(language)"]
        );
        assert_eq!(urls("網址：https://codync.dev，謝謝"), ["https://codync.dev"]);
        assert_eq!(urls("HTTP://A.COM and http://b.io/x_y_z"), ["http://A.COM", "http://b.io/x_y_z"]);
        assert_eq!(urls("https://a.com/**bold**"), ["https://a.com/"]);
    }

    #[test]
    fn a_long_run_of_closing_brackets_is_trimmed_in_one_pass() {
        let started = std::time::Instant::now();
        assert_eq!(urls(&format!("https://a.com/{}", ")".repeat(200_000))), ["https://a.com/"]);
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn skips_what_only_looks_like_an_address() {
        assert_eq!(urls("main.rs README.md codync.dev"), Vec::<String>::new());
        assert_eq!(urls("xhttps://a.com nohttp"), Vec::<String>::new());
        assert_eq!(urls("https:// and www."), Vec::<String>::new());
    }

    #[test]
    fn a_wrapped_address_is_one_link_across_its_lines() {
        let text = underline("go https://example.com/a/very/long/path now", Style::default());
        let lines = super::super::md::wrap(&text, 20, &[Span::raw(" ")], &[Span::raw(" ")]);
        let found = hits(&lines, 20);
        assert!(found.len() >= 2, "{found:?}");
        assert!(found.iter().all(|(_, _, _, url)| url == "https://example.com/a/very/long/path"));
        let short = super::super::md::wrap(&underline("a www.x.io b", Style::default()), 40, &[], &[]);
        assert_eq!(hits(&short, 40), [(0, 2, 8, "https://www.x.io".to_owned())]);
        // Two addresses on lines of their own stay two links.
        let two = [
            Line::from(vec![Span::raw(" "), Span::styled("https://a.com", Style::default().add_modifier(Modifier::UNDERLINED))]),
            Line::from(vec![Span::raw(" "), Span::styled("https://b.com", Style::default().add_modifier(Modifier::UNDERLINED))]),
        ];
        let found: Vec<String> = hits(&two, 14).into_iter().map(|(_, _, _, url)| url).collect();
        assert_eq!(found, ["https://a.com", "https://b.com"]);
    }
}
