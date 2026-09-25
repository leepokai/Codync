//! Text layout for the terminal: a Markdown subset to styled lines, and wrapping
//! that counts display width (CJK and emoji take two cells).

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use super::view::theme;

pub fn width(s: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(s)
}

/// Cuts `s` to at most `max` cells, ending in `…` when it had to cut.
pub fn truncate(s: &str, max: usize) -> String {
    let s = s.lines().next().unwrap_or_default();
    if width(s) <= max {
        return s.to_owned();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw + 1 > max {
            break;
        }
        w += cw;
        out.push(c);
    }
    out.push('…');
    out
}

/// Wraps styled text to `width` cells. Breaks after spaces when it can, anywhere
/// otherwise (so CJK text wraps between characters). `first` / `rest` prefix each line.
pub fn wrap(
    spans: &[Span<'static>],
    width: usize,
    first: &[Span<'static>],
    rest: &[Span<'static>],
) -> Vec<Line<'static>> {
    let cells: Vec<(char, Style)> =
        spans.iter().flat_map(|s| s.content.chars().map(move |c| (c, s.style))).filter(|(c, _)| *c != '\r').collect();
    let mut out = vec![];
    let mut i = 0;
    let mut is_first = true;
    loop {
        let prefix = if is_first { first } else { rest };
        let room = width.saturating_sub(prefix.iter().map(|s| self::width(&s.content)).sum()).max(1);
        let (mut end, mut w, mut brk) = (i, 0, None);
        while end < cells.len() {
            let c = cells[end].0;
            if c == '\n' {
                break;
            }
            let cw = c.width().unwrap_or(0);
            if w + cw > room {
                break;
            }
            w += cw;
            end += 1;
            if c == ' ' {
                brk = Some(end);
            }
        }
        if end == i && end < cells.len() && cells[end].0 != '\n' {
            end += 1; // a glyph wider than the room still has to go somewhere
        }
        let mut next = end;
        if end < cells.len() && !matches!(cells[end].0, '\n' | ' ') {
            if let Some(b) = brk.filter(|&b| b > i) {
                end = b;
                next = b;
            }
        } else if end < cells.len() && cells[end].0 == '\n' {
            next = end + 1; // skip the newline
        }
        let mut line: Vec<Span<'static>> = prefix.to_vec();
        let mut shown = end;
        while shown > i && cells[shown - 1].0 == ' ' && next != cells.len() {
            shown -= 1; // no trailing spaces at a soft break
        }
        push_cells(&mut line, &cells[i..shown]);
        out.push(Line::from(line));
        is_first = false;
        if next >= cells.len() {
            break;
        }
        i = next;
        // Leading spaces after a soft break are dropped.
        if cells[i - 1].0 != '\n' {
            while i < cells.len() && cells[i].0 == ' ' {
                i += 1;
            }
            if i >= cells.len() {
                break;
            }
        }
    }
    out
}

fn push_cells(line: &mut Vec<Span<'static>>, cells: &[(char, Style)]) {
    let mut run = String::new();
    let mut style = None;
    for &(c, s) in cells {
        if style != Some(s) && !run.is_empty() {
            line.push(Span::styled(std::mem::take(&mut run), style.unwrap_or_default()));
        }
        style = Some(s);
        run.push(c);
    }
    if !run.is_empty() {
        line.push(Span::styled(run, style.unwrap_or_default()));
    }
}

/// Inline Markdown: `code`, **bold**, *italic* / _italic_, [text](url).
pub fn inline(text: &str, base: Style) -> Vec<Span<'static>> {
    let t = theme();
    let mut out = vec![];
    let mut run = String::new();
    let (mut bold, mut italic) = (false, false);
    let style = |bold: bool, italic: bool| {
        let mut s = base;
        if bold {
            s = s.add_modifier(Modifier::BOLD);
        }
        if italic {
            s = s.add_modifier(Modifier::ITALIC);
        }
        s
    };
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    let flush = |run: &mut String, out: &mut Vec<Span<'static>>, s: Style| {
        if !run.is_empty() {
            out.push(Span::styled(std::mem::take(run), s));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        if c == '`'
            && let Some(len) = chars[i + 1..].iter().position(|&x| x == '`')
        {
            flush(&mut run, &mut out, style(bold, italic));
            let code: String = chars[i + 1..i + 1 + len].iter().collect();
            out.push(Span::styled(code, t.code));
            i += len + 2;
            continue;
        }
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            flush(&mut run, &mut out, style(bold, italic));
            bold = !bold;
            i += 2;
            continue;
        }
        let word_edge = |j: usize| j == 0 || !chars[j - 1].is_alphanumeric();
        if (c == '*' || (c == '_' && word_edge(i)) || (c == '_' && italic))
            && (italic || chars[i + 1..].contains(&c))
            && chars.get(i + 1).is_some_and(|n| !n.is_whitespace() || italic)
        {
            flush(&mut run, &mut out, style(bold, italic));
            italic = !italic;
            i += 1;
            continue;
        }
        if c == '['
            && let Some(close) = chars[i..].iter().position(|&x| x == ']').map(|p| p + i)
            && chars.get(close + 1) == Some(&'(')
            && let Some(end) = chars[close..].iter().position(|&x| x == ')').map(|p| p + close)
        {
            flush(&mut run, &mut out, style(bold, italic));
            let label: String = chars[i + 1..close].iter().collect();
            out.push(Span::styled(label, style(bold, italic).add_modifier(Modifier::UNDERLINED)));
            i = end + 1;
            continue;
        }
        run.push(c);
        i += 1;
    }
    flush(&mut run, &mut out, style(bold, italic));
    out
}

/// Block Markdown to wrapped lines: headings, lists, quotes, fenced code, rules.
pub fn render(text: &str, width: usize, indent: usize) -> Vec<Line<'static>> {
    let t = theme();
    let pad = Span::raw(" ".repeat(indent));
    let mut out = vec![];
    let mut in_code = false;
    for raw in text.lines() {
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            let code = raw.replace('\t', "    ");
            let room = width.saturating_sub(indent + 2).max(1);
            let body = if self::width(&code) > room { truncate(&code, room) } else { code };
            let fill = room.saturating_sub(self::width(&body));
            out.push(Line::from(vec![
                pad.clone(),
                Span::styled(format!(" {body}{} ", " ".repeat(fill)), t.code_block),
            ]));
            continue;
        }
        if trimmed.is_empty() {
            out.push(Line::default());
            continue;
        }
        if let Some(h) = trimmed.strip_prefix('#') {
            let h = h.trim_start_matches('#').trim();
            out.extend(wrap(
                &inline(h, t.text.add_modifier(Modifier::BOLD)),
                width,
                std::slice::from_ref(&pad),
                std::slice::from_ref(&pad),
            ));
            continue;
        }
        if matches!(trimmed, "---" | "***" | "___") {
            out.push(Line::from(vec![
                pad.clone(),
                Span::styled("─".repeat(width.saturating_sub(indent).min(40)), t.line),
            ]));
            continue;
        }
        if let Some(q) = trimmed.strip_prefix('>') {
            let bar = Span::styled("▎ ", t.dim);
            out.extend(wrap(&inline(q.trim(), t.secondary), width, &[pad.clone(), bar.clone()], &[pad.clone(), bar]));
            continue;
        }
        let lead = raw.len() - trimmed.len();
        let nest = " ".repeat(lead.min(8));
        let bullet = ["- ", "* ", "+ "].iter().find_map(|b| trimmed.strip_prefix(b).map(|r| ("• ".to_owned(), r)));
        let numbered = || {
            let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
            (digits > 0 && trimmed[digits..].starts_with(". "))
                .then(|| (format!("{} ", &trimmed[..=digits]), &trimmed[digits + 2..]))
        };
        if let Some((mark, body)) = bullet.or_else(numbered) {
            let hang = " ".repeat(self::width(&mark));
            let first = [pad.clone(), Span::raw(nest.clone()), Span::styled(mark, t.secondary)];
            let rest = [pad.clone(), Span::raw(nest), Span::raw(hang)];
            out.extend(wrap(&inline(body, t.text), width, &first, &rest));
            continue;
        }
        out.extend(wrap(&inline(trimmed, t.text), width, std::slice::from_ref(&pad), std::slice::from_ref(&pad)));
    }
    while out.last().is_some_and(|l| l.spans.iter().all(|s| s.content.trim().is_empty())) {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: &[Line<'_>]) -> Vec<String> {
        lines.iter().map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect()).collect()
    }

    #[test]
    fn wraps_at_spaces_and_between_cjk() {
        let l = wrap(&[Span::raw("hello brave new world")], 11, &[], &[]);
        assert_eq!(text(&l), ["hello brave", "new world"]);
        let l = wrap(&[Span::raw("你好世界再見")], 5, &[], &[]);
        assert_eq!(text(&l), ["你好", "世界", "再見"]);
    }

    #[test]
    fn keeps_hard_newlines_and_prefixes() {
        let l = wrap(&[Span::raw("a\nb")], 10, &[Span::raw("> ")], &[Span::raw("  ")]);
        assert_eq!(text(&l), ["> a", "  b"]);
    }

    #[test]
    fn inline_code_and_bold() {
        let s = inline("run `cargo` **now**", Style::default());
        let parts: Vec<&str> = s.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(parts, ["run ", "cargo", " ", "now"]);
    }

    #[test]
    fn truncates_by_width() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("你好世界", 5), "你好…");
        assert_eq!(truncate("ok", 5), "ok");
    }
}
