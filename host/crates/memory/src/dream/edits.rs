//! What an audited run writes: each file's new content, and the lines it adds to and takes out of `MEMORY.md`.
//! It only reads the store; the caller writes the files and what goes with them.

use rusqlite::Connection;

use super::{Layer, Message, MoveOut, Plan, Snapshot, message, notes};
use crate::files::{self, Fact, MAIN};
use crate::store::{self, fingerprint};

/// A file a run writes: its new content over what the run saw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    /// The bot whose folder holds it; `None` for the shared one.
    pub bot: Option<String>,
    pub name: String,
    /// `None`: there was no file.
    pub before: Option<String>,
    pub after: String,
}

/// A line a run adds to or takes out of a layer's `MEMORY.md`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub bot: Option<String>,
    pub text: String,
    /// An added line: the transcript id it was learned from.
    pub message: Option<String>,
    /// A review's rewrite: the fact it replaces, whose writer it keeps.
    pub rewrites: Option<Fact>,
}

/// What an audited run writes. Notes come first, so a note takes a line before `MEMORY.md` loses it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edits {
    pub files: Vec<Edit>,
    pub added: Vec<Line>,
    pub removed: Vec<Line>,
}

/// Turns an audited plan into each file's new content. An add the user had forgotten, learned from a message whose
/// memory was forgotten, already in memory, or past the cap is dropped here, with why, and so is a review whose
/// rewrite the user had forgotten. A line moves into a note only when its note takes it and `MEMORY.md` loses it.
pub fn edits(
    conn: &Connection,
    snapshot: &Snapshot,
    plan: &Plan,
    messages: &[Message],
) -> store::Result<(Edits, Vec<String>)> {
    let mut notes = notes::edits(conn, snapshot, plan, messages)?;
    let (mut main, mut rejected, mut stuck) = (Edits::default(), Vec::new(), Vec::new());
    for layer in &snapshot.layers {
        let moved: Vec<&MoveOut> = notes.moved.iter().filter(|m| m.shared == layer.shared).collect();
        match main_edit(conn, snapshot, layer, plan, &moved, messages, &mut rejected)? {
            Some(edits) => {
                main.files.extend(edits.files);
                main.added.extend(edits.added);
                main.removed.extend(edits.removed);
            }
            None if !moved.is_empty() => stuck.push(layer.shared),
            None => {}
        }
    }
    if !stuck.is_empty() {
        // A line its MEMORY.md keeps doesn't go into a note as well.
        let (stay, go): (Vec<MoveOut>, Vec<MoveOut>) =
            plan.move_out.iter().cloned().partition(|m| stuck.contains(&m.shared));
        rejected.extend(stay.iter().map(|m| format!("move \"{}\": {MAIN} wasn't changed", m.line)));
        notes = notes::edits(conn, snapshot, &Plan { move_out: go, ..plan.clone() }, messages)?;
    }
    notes.rejected.append(&mut rejected);
    notes.files.append(&mut main.files);
    Ok((Edits { files: notes.files, added: main.added, removed: main.removed }, notes.rejected))
}

/// A layer's `MEMORY.md` after the run (`moved`: its lines a note took, each topic left as one line pointing to its
/// note), with the lines it gains and loses. `None` when the run changes no line of it (it never only respells one,
/// a final line break say) or the change fails the checks.
fn main_edit(
    conn: &Connection,
    snapshot: &Snapshot,
    layer: &Layer,
    plan: &Plan,
    moved: &[&MoveOut],
    messages: &[Message],
    rejected: &mut Vec<String>,
) -> store::Result<Option<Edits>> {
    let bot = snapshot.layer_bot(layer.shared);
    let line = |text: &str, message: Option<String>, rewrites: Option<Fact>| Line {
        bot: bot.map(str::to_owned),
        text: text.to_owned(),
        message,
        rewrites,
    };
    let before = layer.content.as_deref();
    let mut lines: Vec<String> = before.unwrap_or_default().lines().map(str::to_owned).collect();
    let mut removed = Vec::new();
    for remove in plan.remove.iter().filter(|r| r.shared == layer.shared) {
        removed.extend(take_out(&mut lines, &remove.line));
    }
    for moved in moved {
        removed.extend(take_out(&mut lines, &moved.line));
    }
    let (replaced, pointers) = notes::point(conn, snapshot, layer, moved, &mut lines)?;
    removed.extend(replaced);
    let mut added: Vec<Line> = pointers.iter().map(|text| line(text, None, None)).collect();
    for review in plan.review.iter().filter(|r| r.shared == layer.shared) {
        let Some(fact) = snapshot.fact(layer.shared, &review.line) else { continue };
        // A review's own rewrite is already in the past tense: rewriting it again each day would only churn.
        if store::marked(conn, bot, &reviewed_mark(&review.line))? {
            continue;
        }
        let text = review.text.trim();
        if store::forgotten(conn, bot, text)? {
            rejected.push(format!("review \"{}\": the user had \"{text}\" forgotten", review.line));
            continue;
        }
        if let Some(old) = take_out(&mut lines, &review.line) {
            removed.push(old);
            lines.push(format!("- {text}"));
            added.push(line(text, fact.message.clone(), Some(fact.clone())));
        }
    }
    for add in plan.add.iter().filter(|a| a.shared == layer.shared) {
        let text = add.text.trim();
        let source = message(messages, &add.message);
        let why = if find(&lines, text).is_some() {
            Some("memory already has it")
        } else if forgotten(conn, snapshot, bot, text, source)? {
            Some("the user had it forgotten")
        } else {
            None
        };
        if let Some(why) = why {
            rejected.push(format!("add \"{text}\": {why}"));
            continue;
        }
        lines.push(format!("- {text}"));
        if let Err(refusal) = files::check_change(bot, MAIN, before, &joined(&lines), false) {
            lines.pop();
            rejected.push(format!("add \"{text}\": {refusal}"));
            continue;
        }
        added.push(line(text, source.map(|m| m.id.clone()), None));
    }
    if removed.is_empty() && added.is_empty() {
        return Ok(None);
    }
    let after = joined(&lines);
    if let Err(refusal) = files::check_change(bot, MAIN, before, &after, false) {
        rejected.push(format!("{MAIN}: {refusal}"));
        return Ok(None);
    }
    Ok(Some(Edits {
        files: vec![Edit { bot: bot.map(str::to_owned), name: MAIN.to_owned(), before: layer.content.clone(), after }],
        added,
        removed: removed.iter().map(|l| line(store::unmarked(l), None, None)).collect(),
    }))
}

/// A review's rewrite (`line`) was written: later reviews leave it alone.
pub fn mark_reviewed(conn: &Connection, bot: Option<&str>, line: &str) -> store::Result<()> {
    store::mark(conn, bot, std::iter::once(reviewed_mark(line)))
}

/// Whether the user had `text` forgotten in a layer, or the memory learned from the message it cites (`source`).
pub(super) fn forgotten(
    conn: &Connection,
    snapshot: &Snapshot,
    layer: Option<&str>,
    text: &str,
    source: Option<&Message>,
) -> store::Result<bool> {
    if let Some(message) = source
        && store::spent(conn, &snapshot.bot, &message.id)?
    {
        return Ok(true);
    }
    store::forgotten(conn, layer, text)
}

/// Where the line saying `line` is in `lines`.
fn find(lines: &[String], line: &str) -> Option<usize> {
    let wanted = fingerprint(line);
    lines.iter().position(|l| !l.trim().is_empty() && fingerprint(l) == wanted)
}

/// Takes the line saying `line` out of `lines`.
fn take_out(lines: &mut Vec<String>, line: &str) -> Option<String> {
    find(lines, line).map(|at| lines.remove(at))
}

pub(super) fn joined(lines: &[String]) -> String {
    let text = lines.join("\n");
    if text.trim().is_empty() { String::new() } else { format!("{}\n", text.trim_end()) }
}

/// A mark on a line a time review wrote, which the review leaves alone afterwards.
fn reviewed_mark(line: &str) -> String {
    format!("reviewed:{}", fingerprint(line))
}
