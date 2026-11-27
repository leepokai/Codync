//! A bot's Details page (its tasks' keys: `work.rs`) with its routines, and the routine form (what's typed for
//! when: `when.rs`).

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Confirm, ConfirmAct, Editor, Overlay};
use super::when::{complete_zone, parse_when, when_text};
use super::{DetailRow, DetailsPage, Page, Reply, RoutineField, RoutineForm, ctrl, newline, s};
use crate::routines::local_zone;

impl App {
    pub fn open_details(&mut self) {
        let Some(b) = self.bot().filter(|b| !b.group) else {
            self.flash("Routines belong to a bot, not a group");
            return;
        };
        let bot = b.id.clone();
        self.sheet("routines", json!({"botId": bot}), Reply::Routines(bot.clone()));
        let older = Box::new(self.first_older(&bot));
        self.overlays.push(Overlay::Details(DetailsPage { bot, older, ..Default::default() }));
    }

    /// Loads `bot`'s routines sheet again, if it is open: its runs and follow-ups change with no key pressed.
    pub(in crate::tui) fn reload_details(&mut self, bot: &str) {
        if self.overlays.iter().any(|o| matches!(o, Overlay::Details(l) if l.bot == bot)) {
            self.sheet("routines", json!({"botId": bot}), Reply::Routines(bot.to_owned()));
        }
    }

    pub(in crate::tui) fn details_key(&mut self, mut l: DetailsPage, k: KeyEvent) -> Option<Overlay> {
        if let Some(page) = Page::Details.step(k) {
            self.open_page(page);
            return None;
        }
        // A key of the picked item's own comes first.
        l = match self.work_key(l, k) {
            Ok(page) => return page,
            Err(l) => l,
        };
        let bot = l.bot.clone();
        let (work, max_threads) = self.box_of(&bot);
        let work = work.to_vec();
        let n = l.rows(&work, max_threads).len();
        match (k.code, l.picked(&work, max_threads)) {
            (KeyCode::Esc | KeyCode::Char('q' | 'R'), _) => return None,
            // The row stays until the host answers: a cancel that failed leaves a follow-up that still happens.
            (KeyCode::Char('x'), Some(DetailRow::FollowUp(f))) => {
                let body = json!({"botId": bot, "id": f["id"]});
                self.sheet("cancelFollowUp", body, Reply::FollowUpCancelled(bot.clone()));
            }
            (KeyCode::Enter | KeyCode::Char('e' | ' ' | 'r' | 'w' | 'W'), Some(DetailRow::FollowUp(_))) => {
                self.flash("A follow-up is the bot's own: x cancels it");
            }
            (KeyCode::Down | KeyCode::Char('j'), _) => {
                l.cursor = (l.cursor + 1).min(n.saturating_sub(1));
                self.more_older(&mut l, n);
            }
            (KeyCode::Up | KeyCode::Char('k'), _) => l.cursor = l.cursor.saturating_sub(1),
            (KeyCode::Char('n'), _) => {
                self.overlays.push(Overlay::Details(l));
                self.overlays.push(Overlay::Routine(Box::new(RoutineForm {
                    bot,
                    id: None,
                    name: Editor::default(),
                    instruction: Editor::default(),
                    when: Editor::default(),
                    folder: self.routine_folder.then(Editor::default),
                    original: Value::Null,
                    original_text: String::new(),
                    field: RoutineField::Name,
                    error: None,
                    saving: false,
                })));
                return None;
            }
            (KeyCode::Enter | KeyCode::Char('e'), Some(DetailRow::Routine(r))) => {
                let triggers = r["triggers"].clone();
                let text = when_text(&triggers, &local_zone());
                let original_text = r["triggerDescriptions"]
                    .as_array()
                    .map(|d| d.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" · "))
                    .unwrap_or_default();
                let form = RoutineForm {
                    bot,
                    id: Some(s(r, "id")),
                    name: Editor::with(&s(r, "name")),
                    instruction: Editor::with(&s(r, "instruction")),
                    when: Editor::with(text.as_deref().unwrap_or_default()),
                    folder: self.routine_folder.then(|| Editor::with(r["folder"].as_str().unwrap_or_default())),
                    original: if text.is_some() { Value::Null } else { triggers },
                    original_text,
                    field: RoutineField::Name,
                    error: None,
                    saving: false,
                };
                self.overlays.push(Overlay::Details(l));
                self.overlays.push(Overlay::Routine(Box::new(form)));
                return None;
            }
            (KeyCode::Char(' '), Some(DetailRow::Routine(r))) => {
                let (id, on) = (r["id"].clone(), !r["enabled"].as_bool().unwrap_or(false));
                self.sheet(
                    "setRoutineEnabled",
                    json!({"botId": bot, "id": id, "enabled": on}),
                    Reply::Routines(bot.clone()),
                );
                if let Some(item) = l.routines.iter_mut().flatten().find(|item| item["id"] == id) {
                    item["enabled"] = on.into();
                }
            }
            (KeyCode::Char('r'), Some(DetailRow::Routine(r))) => {
                self.sheet("runRoutine", json!({"botId": bot, "id": r["id"]}), Reply::Routines(bot.clone()));
                self.flash(&format!("Running {} now", s(r, "name")));
            }
            (KeyCode::Char('w' | 'W'), Some(DetailRow::Routine(r))) if !has_webhook(r) => {
                self.flash("Only a routine that runs on a webhook has a URL and key");
            }
            (KeyCode::Char('w'), Some(DetailRow::Routine(r))) => {
                self.sheet("routineWebhook", json!({"botId": bot, "id": r["id"]}), Reply::Webhook);
            }
            (KeyCode::Char('W'), Some(DetailRow::Routine(r))) => {
                let c = Confirm {
                    title: format!("Replace the webhook key of {}?", s(r, "name")),
                    detail: "Senders using the current key stop working.".into(),
                    note: "The new one is copied in a curl command.".into(),
                    button: "Replace",
                    act: ConfirmAct::RotateRoutineKey(bot, s(r, "id")),
                };
                self.overlays.push(Overlay::Details(l));
                self.overlays.push(Overlay::Confirm(c));
                return None;
            }
            (KeyCode::Char('x'), Some(DetailRow::Routine(r))) => {
                let c = Confirm {
                    title: format!("Delete the routine {}?", s(r, "name")),
                    detail: "It stops running.".into(),
                    note: "What it already posted stays in the chat.".into(),
                    button: "Delete",
                    act: ConfirmAct::DeleteRoutine(bot, s(r, "id")),
                };
                self.overlays.push(Overlay::Details(l));
                self.overlays.push(Overlay::Confirm(c));
                return None;
            }
            _ => {}
        }
        Some(Overlay::Details(l))
    }

    pub(in crate::tui) fn routine_key(&mut self, mut f: Box<RoutineForm>, k: KeyEvent) -> Option<Overlay> {
        use RoutineField::When;
        // Tab finishes a time zone being typed after the schedule before it moves on.
        if k.code == KeyCode::Tab
            && f.field == When
            && let Some(done) = complete_zone(&f.when.text, crate::store::now_ms())
        {
            f.when = Editor::with(&done);
            return Some(Overlay::Routine(f));
        }
        match k.code {
            KeyCode::Esc => return None,
            _ if newline(k) && f.field == RoutineField::Instruction => f.instruction.insert("\n"),
            KeyCode::Enter => self.save_routine(&mut f),
            KeyCode::Char('s') if ctrl(k) => self.save_routine(&mut f),
            KeyCode::Tab | KeyCode::Down => f.step(1),
            KeyCode::BackTab | KeyCode::Up => f.step(-1),
            _ => {
                if let Some(ed) = f.editor() {
                    ed.key(k);
                }
            }
        }
        Some(Overlay::Routine(f))
    }

    fn save_routine(&mut self, f: &mut RoutineForm) {
        if f.saving {
            return;
        }
        let (name, instruction) = (f.name.text.trim(), f.instruction.text.trim());
        if name.is_empty() || instruction.is_empty() {
            f.error = Some("A routine needs a name and what to do.".into());
            return;
        }
        let triggers = if f.when.text.trim().is_empty() && !f.original.is_null() {
            f.original.clone()
        } else {
            match parse_when(&f.when.text, &local_zone(), crate::store::now_ms()) {
                Ok(t) => json!([t]),
                Err(e) => {
                    f.error = Some(e);
                    f.field = RoutineField::When;
                    return;
                }
            }
        };
        let body = f.body(&triggers);
        f.saving = true;
        f.error = None;
        self.sheet("saveRoutine", body, Reply::RoutineSaved);
    }
}

impl RoutineForm {
    /// The fields in the order tab visits them: the folder only on a host that has it.
    fn fields(&self) -> Vec<RoutineField> {
        let mut out = vec![RoutineField::Name, RoutineField::Instruction];
        if self.folder.is_some() {
            out.push(RoutineField::Folder);
        }
        out.push(RoutineField::When);
        out
    }

    fn step(&mut self, by: isize) {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.field).unwrap_or(0);
        self.field = fields[(at.cast_signed() + by).rem_euclid(fields.len().cast_signed()).cast_unsigned()];
    }

    pub(in crate::tui) fn editor(&mut self) -> Option<&mut Editor> {
        match self.field {
            RoutineField::Name => Some(&mut self.name),
            RoutineField::Instruction => Some(&mut self.instruction),
            RoutineField::Folder => self.folder.as_mut(),
            RoutineField::When => Some(&mut self.when),
        }
    }

    /// `saveRoutine`'s body. The folder goes whole each time, as typed (a space at either end is part of a name):
    /// blank clears it (null).
    fn body(&self, triggers: &Value) -> Value {
        let mut body = json!({"botId": self.bot, "name": self.name.text.trim(),
            "instruction": self.instruction.text.trim(), "triggers": triggers});
        if let Some(id) = &self.id {
            body["id"] = id.clone().into();
        }
        if let Some(folder) = &self.folder {
            let path = &folder.text;
            body["folder"] = if path.trim().is_empty() { Value::Null } else { path.as_str().into() };
        }
        body
    }
}

/// A routine with a webhook (or event) trigger, which has a URL and key.
fn has_webhook(r: &Value) -> bool {
    r["triggers"].as_array().into_iter().flatten().any(|t| matches!(t["type"].as_str(), Some("webhook" | "event")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(folder: Option<&str>) -> RoutineForm {
        RoutineForm {
            bot: "a".into(),
            id: Some("r".into()),
            name: Editor::with("Digest"),
            instruction: Editor::with("Sum up"),
            when: Editor::default(),
            folder: folder.map(Editor::with),
            original: Value::Null,
            original_text: String::new(),
            field: RoutineField::Name,
            error: None,
            saving: false,
        }
    }

    #[test]
    fn the_folder_goes_as_a_path_or_null_and_not_at_all_on_a_host_without_it() {
        let triggers = json!([]);
        assert_eq!(form(Some("/work/site ")).body(&triggers)["folder"], "/work/site ");
        let cleared = form(Some("  ")).body(&triggers);
        assert!(cleared.get("folder").is_some_and(Value::is_null), "empty: back to the bot's folder");
        assert!(form(None).body(&triggers).get("folder").is_none(), "an older host keeps what it has");
    }

    #[test]
    fn tab_visits_the_folder_only_when_the_host_has_it() {
        use RoutineField::{Folder, Instruction, Name, When};
        let mut f = form(Some(""));
        let mut seen = Vec::new();
        for _ in 0..4 {
            f.step(1);
            seen.push(f.field);
        }
        assert_eq!(seen, [Instruction, Folder, When, Name]);
        f.step(-1);
        assert_eq!(f.field, When);
        let mut old = form(None);
        old.field = Instruction;
        old.step(1);
        assert_eq!(old.field, When);
    }
}
