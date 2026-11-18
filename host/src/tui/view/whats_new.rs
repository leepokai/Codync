//! The What's new sheet: each highlight's name, what it does and the link to its video, then the smaller changes.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};

use super::super::app::WhatsNew;
use super::super::l10n::tr;
use super::super::md;
use super::buffer::{centered, scroll_sheet, u};
use super::style::theme;

pub(super) fn whats_new(buf: &mut Buffer, area: Rect, s: &WhatsNew) {
    let t = theme();
    let most = centered(area, 72, area.height.saturating_sub(4));
    let width = usize::from(most.width.saturating_sub(4));
    let pad = [Span::raw(" ")];
    let wrapped = |text: String| md::wrap(&[Span::styled(text, t.secondary)], width, &pad, &pad);
    // A row apart, like the apps' stories.
    let mut blocks: Vec<Vec<Line>> = Vec::new();
    for a in &s.due {
        for f in &a.features {
            let mut title = vec![Span::styled(format!(" {}", tr(&f.title)), t.bold)];
            if f.beta {
                title.push(Span::styled(format!(" {}", tr("Beta")), t.secondary));
            }
            let mut block = vec![Line::from(title)];
            block.extend(wrapped(tr(&f.body).into_owned()));
            // The full video: terminals open a link they recognize.
            if let Some(film) = &f.film {
                block.extend(wrapped(format!("▶ {film}")));
            }
            blocks.push(block);
        }
        if !a.notes.is_empty() {
            let mut block = vec![Line::from(Span::styled(format!(" {}", tr("Also in this release")), t.bold))];
            block.extend(a.notes.iter().flat_map(|note| wrapped(tr(note).into_owned())));
            blocks.push(block);
        }
    }
    let lines = blocks.join(&Line::default());
    let title = match s.due.first() {
        Some(a) => format!("{} {}", tr("What's new"), a.version),
        None => tr("What's new").into_owned(),
    };
    // Borders and the footer around the lines, as tall as they need.
    let r = centered(area, most.width, u(lines.len() + 4).min(most.height));
    scroll_sheet(buf, r, &title, &lines, &s.scroll);
}
