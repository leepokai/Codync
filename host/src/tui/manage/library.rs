//! A bot's Library (`docs/features/artifacts.md`): the artifacts it published and the files shared in its chat,
//! newest first; also the newest titles artifact cards show, and opening or saving an artifact.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

use super::super::app::{App, Editor, Entry, Overlay};
use super::super::net::artifacts::ArtifactTo;
use super::{Fields, Input, Library, Page, Reply, Submit};
use crate::chat::artifacts::Artifact;
use crate::chat::files::SharedFile;

/// One row of a Library (`library {botId}`).
#[derive(Clone, Debug)]
pub enum LibraryItem {
    Artifact(Artifact),
    /// A file the bot shared (`readFile`).
    File {
        entry: String,
        file: SharedFile,
        at: i64,
    },
    /// A file the user attached (`readUpload`).
    Attachment {
        id: String,
        name: String,
        size: u64,
        at: i64,
    },
}

/// The newest title of each artifact (artifact id → title), and the bots whose Library was read for them.
#[derive(Default)]
pub struct ArtifactTitles {
    titles: HashMap<String, String>,
    asked: HashSet<String>,
}

impl LibraryItem {
    fn parse(v: &Value) -> Option<Self> {
        let at = v["at"].as_i64().unwrap_or_default();
        let file = &v["file"];
        match v["kind"].as_str()? {
            "artifact" => serde_json::from_value(v["artifact"].clone()).ok().map(Self::Artifact),
            "file" => Some(Self::File {
                entry: v["entryId"].as_str()?.to_owned(),
                file: serde_json::from_value(file.clone()).ok()?,
                at,
            }),
            "attachment" => Some(Self::Attachment {
                id: file["id"].as_str()?.to_owned(),
                name: file["name"].as_str()?.to_owned(),
                size: file["size"].as_u64().unwrap_or_default(),
                at,
            }),
            _ => None,
        }
    }

    /// When it was published or shared (epoch ms): the list's order.
    pub fn at(&self) -> i64 {
        match self {
            Self::Artifact(a) => a.updated_at,
            Self::File { at, .. } | Self::Attachment { at, .. } => *at,
        }
    }
}

impl App {
    pub fn open_library(&mut self) {
        let Some(b) = self.bot().filter(|b| !b.group) else {
            self.flash("The Library belongs to a bot, not a group");
            return;
        };
        let bot = b.id.clone();
        self.sheet("library", json!({"botId": bot}), Reply::Library(bot.clone()));
        self.overlays.push(Overlay::Library(Library { bot, items: None, cursor: 0 }));
    }

    pub(in crate::tui) fn library_key(&mut self, mut l: Library, k: KeyEvent) -> Option<Overlay> {
        if let Some(page) = Page::Library.step(k) {
            self.open_page(page);
            return None;
        }
        let n = l.items.as_ref().map_or(0, Vec::len);
        let current = l.items.as_ref().and_then(|v| v.get(l.cursor)).cloned();
        let bot = l.bot.clone();
        match (k.code, current) {
            (KeyCode::Esc | KeyCode::Char('q' | 'L'), _) => return None,
            (KeyCode::Down | KeyCode::Char('j'), _) => l.cursor = (l.cursor + 1).min(n.saturating_sub(1)),
            (KeyCode::Up | KeyCode::Char('k'), _) => l.cursor = l.cursor.saturating_sub(1),
            (KeyCode::Enter | KeyCode::Char('o'), Some(LibraryItem::Artifact(a))) => {
                self.fetch_artifact(&bot, &a.id, ArtifactTo::Browser);
            }
            (KeyCode::Enter | KeyCode::Char('o'), Some(_)) => self.flash("f saves it to Downloads"),
            (KeyCode::Char('f'), Some(item)) => self.save_item(&bot, item),
            (KeyCode::Char('r'), Some(LibraryItem::Artifact(a))) => {
                self.overlays.push(Overlay::Library(l));
                self.overlays.push(Overlay::Fields(Box::new(rename_fields(bot, a.id, &a.title))));
                return None;
            }
            (KeyCode::Char('r'), Some(_)) => self.flash("Only artifacts can be renamed"),
            _ => {}
        }
        Some(Overlay::Library(l))
    }

    /// `f`: saves the row to Downloads, or stops the download running.
    fn save_item(&mut self, bot: &str, item: LibraryItem) {
        if self.cancel_download() {
            return;
        }
        match item {
            LibraryItem::Artifact(a) => self.fetch_artifact(bot, &a.id, ArtifactTo::Downloads),
            LibraryItem::File { entry, file, .. } => self.download_shared(entry, file),
            LibraryItem::Attachment { id, name, .. } => self.save_uploads(bot, vec![(id, name)]),
        }
    }

    /// Reads the artifact's HTML into the browser or Downloads, in place of any download running.
    pub(in crate::tui) fn fetch_artifact(&mut self, bot: &str, artifact: &str, to: ArtifactTo) {
        self.download = Some(self.client.fetch_artifact(bot.to_owned(), artifact.to_owned(), to, self.tx.clone()));
        self.flash(match to {
            ArtifactTo::Browser => "Opening the artifact… · f cancels",
            ArtifactTo::Downloads => "Saving the artifact… · f cancels",
        });
    }

    /// A card's title: the newest one known (a rename or a publish since), else the card's own.
    pub(in crate::tui) fn artifact_title(&self, e: &Entry) -> Option<String> {
        let card = &e.data["artifact"];
        let id = card["id"].as_str()?;
        let title = self
            .artifact_titles
            .titles
            .get(id)
            .map_or_else(|| card["title"].as_str().unwrap_or_default(), String::as_str);
        Some(title.to_owned())
    }

    /// A new entry in `bot`'s chat. One that adds to its Library (an artifact card, a message with files)
    /// reads an open Library again. An artifact card also reads it once a run, so every card of an artifact
    /// shows its newest title (a card keeps the title it was posted with).
    pub(in crate::tui) fn entry_added(&mut self, bot: &str, e: &Entry) {
        let open = self.overlays.iter().any(|o| matches!(o, Overlay::Library(l) if l.bot == bot));
        let first_card = e.artifact().is_some() && !self.artifact_titles.asked.contains(bot);
        if open && adds_to_library(e) || first_card {
            self.artifact_titles.asked.insert(bot.to_owned());
            self.sheet("library", json!({"botId": bot}), Reply::Library(bot.to_owned()));
        }
    }

    /// A fetched Library: shown, and its artifacts' titles kept for their cards.
    pub(in crate::tui) fn library_loaded(&mut self, bot: &str, items: &[Value]) {
        let items: Vec<LibraryItem> = items.iter().filter_map(LibraryItem::parse).collect();
        for item in &items {
            if let LibraryItem::Artifact(a) = item {
                self.artifact_titles.titles.insert(a.id.clone(), a.title.clone());
            }
        }
        for o in &mut self.overlays {
            if let Overlay::Library(l) = o
                && l.bot == bot
            {
                l.cursor = l.cursor.min(items.len().saturating_sub(1));
                l.items = Some(items.clone());
            }
        }
    }

    /// An `artifact` event (a publish or a rename): its cards' title, and the open Library lists it first.
    pub(in crate::tui) fn artifact_changed(&mut self, v: &Value) {
        let Ok(artifact) = serde_json::from_value::<Artifact>(v.clone()) else { return };
        self.artifact_titles.titles.insert(artifact.id.clone(), artifact.title.clone());
        for o in &mut self.overlays {
            let Overlay::Library(l) = o else { continue };
            let Some(items) = l.items.as_mut().filter(|_| l.bot == artifact.bot_id) else { continue };
            items.retain(|x| !matches!(x, LibraryItem::Artifact(a) if a.id == artifact.id));
            items.push(LibraryItem::Artifact(artifact.clone()));
            items.sort_by_key(|x| std::cmp::Reverse(x.at()));
        }
    }
}

/// Whether an entry puts something in its chat's Library: an artifact card, or a message with files the
/// bot shared or the user attached.
fn adds_to_library(e: &Entry) -> bool {
    e.artifact().is_some()
        || ["files", "attachments"].iter().any(|k| e.data[*k].as_array().is_some_and(|a| !a.is_empty()))
}

/// `r` on an artifact: its title in a field of its own.
fn rename_fields(bot: String, artifact: String, title: &str) -> Fields {
    Fields {
        title: "Rename artifact".into(),
        note: String::new(),
        choices: vec![],
        choice: 0,
        inputs: vec![Input {
            name: "title".into(),
            label: "Artifact title".into(),
            secret: false,
            required: true,
            multiline: false,
            placeholder: String::new(),
            ed: Editor::with(title),
        }],
        cursor: 0,
        error: None,
        saving: false,
        submit: Submit::RenameArtifact { bot, artifact },
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::app::Msg;
    use super::*;

    fn artifact(id: &str, title: &str, updated_at: i64) -> Value {
        json!({"id": id, "botId": "bot", "title": title, "size": 1, "sha256": "0".repeat(64),
               "createdAt": 0, "updatedAt": updated_at})
    }

    #[test]
    fn artifact_cards_and_messages_with_files_add_to_the_library() {
        let entry = |kind: &str, data: Value| {
            Entry::parse(&json!({"id": "e", "botId": "bot", "seq": 1, "kind": kind, "data": data})).expect("parses").1
        };
        let card = json!({"text": "Artifact: A", "final": true, "artifact": {"id": "a", "title": "A", "updatedAt": 1}});
        assert!(adds_to_library(&entry("agent", card)));
        let file = json!({"name": "a.csv", "id": "f", "size": 1, "sha256": ""});
        assert!(adds_to_library(&entry("agent", json!({"text": "", "final": true, "files": [file]}))));
        let attachment = json!({"id": "u", "name": "b.png", "size": 1});
        assert!(adds_to_library(&entry("user", json!({"text": "look", "attachments": [attachment]}))));
        assert!(!adds_to_library(&entry("user", json!({"text": "hi", "attachments": []}))));
        assert!(!adds_to_library(&entry("agent", json!({"text": "Done.", "final": true}))));
        assert!(!adds_to_library(&entry("tool", json!({"artifact": {"id": "a"}}))), "only a final reply is a card");
    }

    #[tokio::test]
    async fn artifact_events_rename_cards_and_move_the_artifact_to_the_top() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(crate::tui::net::Client::new("http://127.0.0.1:1", None), tx, String::new());
        app.on_msg(Msg::Event(json!({"type": "bot", "bot": {"id": "bot", "name": "Artifacts"}})));
        let card = Entry::parse(&json!({"id": "e", "botId": "bot", "seq": 1, "kind": "agent",
            "data": {"text": "Artifact: Old", "final": true, "artifact": {"id": "a", "title": "Old", "updatedAt": 1}}}))
        .expect("a card parses")
        .1;
        assert_eq!(app.artifact_title(&card).as_deref(), Some("Old"));
        app.overlays.push(Overlay::Library(Library { bot: "bot".into(), items: None, cursor: 0 }));
        let file = json!({"id": "x", "name": "a.csv", "size": 9, "sha256": "0".repeat(64)});
        app.library_loaded(
            "bot",
            &[
                json!({"kind": "file", "at": 3, "entryId": "f", "file": file}),
                json!({"kind": "attachment", "at": 2, "entryId": "u", "file": {"id": "y", "name": "b.png", "size": 4}}),
                json!({"kind": "artifact", "at": 1, "artifact": artifact("a", "Old", 1)}),
                json!({"kind": "unknown", "at": 0}),
            ],
        );
        app.on_msg(Msg::Event(json!({"type": "artifact", "artifact": artifact("a", "New", 9)})));
        assert_eq!(app.artifact_title(&card).as_deref(), Some("New"), "the newest title");
        let Some(Overlay::Library(l)) = app.overlays.last() else { panic!("the Library stays open") };
        let rows: Vec<String> = l
            .items
            .iter()
            .flatten()
            .map(|item| match item {
                LibraryItem::Artifact(a) => format!("artifact {}", a.title),
                LibraryItem::File { file, .. } => format!("file {}", file.name),
                LibraryItem::Attachment { name, .. } => format!("attachment {name}"),
            })
            .collect();
        assert_eq!(rows, ["artifact New", "file a.csv", "attachment b.png"]);
    }
}
