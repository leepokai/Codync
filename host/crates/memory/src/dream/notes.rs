//! What dreaming adds to notes: a line that cites the user's words (`append`), or a line moved out of a nearly full
//! `MEMORY.md` (`moveOut`), which keeps one line summing the topic up and pointing to the note ([`pointer`]). It
//! only appends; it never rewrites a note.

use rusqlite::Connection;
use serde::Deserialize;
use std::collections::BTreeMap;

use super::{Edit, Layer, Message, Plan, Snapshot, cite};
use crate::files;
use crate::store::{self, unmarked};
use crate::{check, tokens};

/// The most one appended line may hold, in estimated tokens.
pub const NOTE_CAP: usize = 300;

/// A line for a note, cited like an add.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Append {
    pub file: String,
    #[serde(default)]
    pub shared: bool,
    pub text: String,
    #[serde(default)]
    pub quote: String,
    #[serde(default)]
    pub message: String,
    /// When to read the file; needed when it is new.
    #[serde(default)]
    pub description: String,
}

/// A line moved out of a nearly full `MEMORY.md` into a note of the same layer.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MoveOut {
    pub line: String,
    #[serde(default)]
    pub shared: bool,
    pub file: String,
    #[serde(default)]
    pub description: String,
    /// What every reply still needs from the topic: `MEMORY.md` keeps it as the line pointing to the note
    /// ([`pointer`]). Without one, a note it doesn't point to yet is summed up by its description, else its name.
    #[serde(default)]
    pub summary: String,
}

/// The line `MEMORY.md` keeps for a note, the way a project's `CLAUDE.md` points into its `docs/`: what every reply
/// needs from the topic, then where the rest is (`Allergic to peanuts → health/allergies.md`).
fn pointer(summary: &str, note: &str) -> String {
    format!("{} → {note}", summary.trim())
}

/// The note a `MEMORY.md` line points to ([`pointer`]), when it is such a line.
fn points_to(line: &str) -> Option<&str> {
    let (_, note) = unmarked(line).trim_end().rsplit_once(" → ")?;
    (files::valid_name(note) && note != files::MAIN).then_some(note)
}

/// Leaves a layer's `MEMORY.md` (`lines`) one line pointing to each note its moved lines went into ([`pointer`]):
/// the run's summary of the topic, else what the note is about. A line already pointing to the note stays, unless
/// the run summed the topic up anew and the user didn't write that line: it is then rewritten in place. A pointer
/// that would fail `MEMORY.md`'s checks (no room, a line too long) or that the user had forgotten gives way to the
/// next, or is left out, as the index lists the note anyway, so it never costs the run its other changes. Returns
/// the lines taken out and the texts added.
pub(super) fn point(
    conn: &Connection,
    snapshot: &Snapshot,
    layer: &Layer,
    moved: &[&MoveOut],
    lines: &mut Vec<String>,
) -> store::Result<(Vec<String>, Vec<String>)> {
    let bot = snapshot.layer_bot(layer.shared);
    let passes = |lines: &[String]| {
        files::check_change(bot, files::MAIN, layer.content.as_deref(), &super::edits::joined(lines), false).is_ok()
    };
    let forgotten = |text: &str| store::forgotten(conn, bot, text);
    let (mut removed, mut added) = (Vec::new(), Vec::new());
    for (i, first) in moved.iter().enumerate() {
        let note = first.file.as_str();
        if moved[..i].iter().any(|m| m.file == note) {
            continue;
        }
        let summary = moved[i..].iter().filter(|m| m.file == note).map(|m| m.summary.trim()).find(|s| !s.is_empty());
        let Some(at) = lines.iter().position(|l| points_to(l) == Some(note)) else {
            for summed in summary.map(str::to_owned).into_iter().chain(about(snapshot, first)) {
                let text = pointer(&summed, note);
                if forgotten(&text)? {
                    continue;
                }
                lines.push(format!("- {text}"));
                if passes(lines) {
                    added.push(text);
                    break;
                }
                lines.pop();
            }
            continue;
        };
        let Some(text) = summary.map(|s| pointer(s, note)) else { continue };
        let old = unmarked(&lines[at]);
        if old == text || snapshot.fact(layer.shared, old).is_some_and(|f| f.author.is_users()) || forgotten(&text)? {
            continue;
        }
        let was = std::mem::replace(&mut lines[at], format!("- {text}"));
        if passes(lines) {
            removed.push(was);
            added.push(text);
        } else {
            lines[at] = was;
        }
    }
    Ok((removed, added))
}

/// What a note is about, best first: the move's description (a new note's), the note's own, its name.
fn about(snapshot: &Snapshot, m: &MoveOut) -> Vec<String> {
    let given = Some(m.description.trim().to_owned()).filter(|d| !d.is_empty());
    let known = snapshot.note(m.shared, &m.file).and_then(|f| f.description.clone());
    let name = m.file.trim_end_matches(".md").rsplit('/').next().map(str::to_owned);
    [given, known, name].into_iter().flatten().collect()
}

/// Keeps the appends and moves the rules allow into `kept`, and says why the others were dropped.
pub(super) fn audit(
    snapshot: &Snapshot,
    messages: &[Message],
    appends: Vec<Append>,
    moves: Vec<MoveOut>,
    kept: &mut Plan,
    rejected: &mut Vec<String>,
) {
    // A plan may describe a new note once: every line it sends there shares that description.
    let mut described: BTreeMap<(bool, String), String> = BTreeMap::new();
    let named = appends.iter().map(|a| (a.shared, &a.file, &a.description));
    for (shared, file, description) in named.chain(moves.iter().map(|m| (m.shared, &m.file, &m.description))) {
        if !description.trim().is_empty() {
            described.entry((shared && snapshot.shared, file.clone())).or_insert_with(|| description.trim().to_owned());
        }
    }
    let describe = |shared: bool, file: &str, description: &mut String| {
        if description.trim().is_empty()
            && let Some(known) = described.get(&(shared, file.to_owned()))
        {
            description.clone_from(known);
        }
    };
    for mut append in appends {
        append.shared &= snapshot.shared;
        describe(append.shared, &append.file, &mut append.description);
        let verdict = target(snapshot, append.shared, &append.file, &append.description)
            .and_then(|()| text(&append.text))
            .and_then(|()| cite(messages, &append.quote, &append.message).map(drop));
        match verdict {
            Ok(()) => kept.append.push(append),
            Err(why) => rejected.push(format!("append to {}: {why}", append.file)),
        }
    }
    for mut moved in moves {
        moved.shared &= snapshot.shared;
        describe(moved.shared, &moved.file, &mut moved.description);
        let verdict = (|| {
            // Only the layer that is nearly full gets tidied: the other keeps its lines, the user's above all.
            if !snapshot.layer_full(moved.shared) {
                return Err("MEMORY.md isn't full");
            }
            // Even a line the user wrote moves: word for word, into a note of the same layer.
            snapshot.fact(moved.shared, &moved.line).ok_or("no such line in MEMORY.md")?;
            if points_to(&moved.line).is_some() {
                return Err("it points to a note");
            }
            target(snapshot, moved.shared, &moved.file, &moved.description)?;
            if moved.summary.trim().is_empty() {
                return Ok(());
            }
            check::text(&pointer(&moved.summary, &moved.file)).map_err(check::Refusal::message)
        })();
        match verdict {
            Ok(()) => kept.move_out.push(moved),
            Err(why) => rejected.push(format!("move \"{}\": {why}", moved.line)),
        }
    }
}

/// A note name the folders take; a new note also needs a description that passes the checks.
fn target(snapshot: &Snapshot, shared: bool, file: &str, description: &str) -> Result<(), &'static str> {
    if !files::valid_name(file) || file == files::MAIN {
        return Err("the file name isn't lowercase-with-dashes.md, in up to three topic folders");
    }
    if snapshot.note(shared, file).is_none() && (description.trim().is_empty() || check::content(description).is_err())
    {
        return Err("a new note needs a description");
    }
    Ok(())
}

fn text(text: &str) -> Result<(), &'static str> {
    if text.trim().is_empty() {
        return Err("the note is empty");
    }
    if tokens::estimate(text) > NOTE_CAP {
        return Err("the note is too long");
    }
    check::content(text).map_err(check::Refusal::message)
}

/// What a run writes to notes.
pub(super) struct Notes {
    pub(super) files: Vec<Edit>,
    /// The moves whose note took the line, for `MEMORY.md` to lose it.
    pub(super) moved: Vec<MoveOut>,
    pub(super) rejected: Vec<String>,
}

/// Each note's new content: its lines appended, a new note starting with frontmatter. A line the user had
/// forgotten, or learned from a message whose memory was, stays out; a note past its checks is left as it was.
pub(super) fn edits(conn: &Connection, snapshot: &Snapshot, plan: &Plan, messages: &[Message]) -> store::Result<Notes> {
    let mut out = Notes { files: Vec::new(), moved: Vec::new(), rejected: Vec::new() };
    // (layer, file) → (description, lines), in a stable order.
    let mut notes: BTreeMap<(Option<String>, String), (String, Vec<String>)> = BTreeMap::new();
    for append in &plan.append {
        let layer = snapshot.layer_bot(append.shared);
        let text = append.text.trim();
        if super::edits::forgotten(conn, snapshot, layer, text, super::message(messages, &append.message))? {
            out.rejected.push(format!("append to {}: the user had it forgotten", append.file));
            continue;
        }
        let key = (layer.map(str::to_owned), append.file.clone());
        let note = notes.entry(key).or_insert_with(|| (append.description.trim().to_owned(), Vec::new()));
        note.1.push(text.to_owned());
    }
    for moved in &plan.move_out {
        let key = (snapshot.layer_bot(moved.shared).map(str::to_owned), moved.file.clone());
        let note = notes.entry(key).or_insert_with(|| (moved.description.trim().to_owned(), Vec::new()));
        note.1.push(unmarked(&moved.line).to_owned());
    }
    for ((bot, name), (description, lines)) in notes {
        let before = files::current(conn, bot.as_deref(), &name)?;
        let mut after = match &before {
            Some(text) => format!("{}\n", text.trim_end()),
            None => format!("---\ndescription: {description}\n---\n"),
        };
        for line in &lines {
            let line = if line.starts_with(['-', '#', '*']) { line.clone() } else { format!("- {line}") };
            after.push_str(&line);
            after.push('\n');
        }
        match files::check_change(bot.as_deref(), &name, before.as_deref(), &after, false) {
            Ok(()) => {
                let into = |m: &&MoveOut| snapshot.layer_bot(m.shared) == bot.as_deref() && m.file == name;
                out.moved.extend(plan.move_out.iter().filter(into).cloned());
                out.files.push(Edit { bot, name, before, after });
            }
            Err(refusal) => out.rejected.push(format!("note {name}: {refusal}")),
        }
    }
    Ok(out)
}
