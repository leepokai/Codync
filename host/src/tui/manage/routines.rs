//! A bot's routines: the list and the form (what's typed for when: `when.rs`).

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{After, App, Confirm, ConfirmAct, Editor, FIELDS, Field, Focus, Overlay};
use super::when::{complete_zone, parse_when, when_text};
use super::{Page, Reply, RoutineField, RoutineForm, RoutineList, ctrl, newline, s};
use crate::routines::local_zone;

impl App {
    pub fn open_routines(&mut self) {
        let Some(b) = self.bot().filter(|b| !b.group) else {
            self.flash("Routines belong to a bot, not a group");
            return;
        };
        let bot = b.id.clone();
        self.sheet("routines", json!({"botId": bot}), Reply::Routines(bot.clone()));
        self.overlays.push(Overlay::Routines(RoutineList { bot, items: None, runs: Vec::new(), cursor: 0 }));
    }

    pub(in crate::tui) fn routines_key(&mut self, mut l: RoutineList, k: KeyEvent) -> Option<Overlay> {
        if let Some(page) = Page::Details.step(k) {
            self.open_page(page);
            return None;
        }
        let n = l.items.as_ref().map_or(0, Vec::len);
        let current = l.items.as_ref().and_then(|v| v.get(l.cursor)).cloned();
        let bot = l.bot.clone();
        match (k.code, current) {
            (KeyCode::Esc | KeyCode::Char('q' | 'R'), _) => return None,
            // The agent, model and instructions shown above the routines are edited in Settings.
            (KeyCode::Char('i'), _) => {
                self.open_page(Page::Settings);
                if let Some(Overlay::Form(f)) = self.overlays.last_mut() {
                    f.field = FIELDS.iter().position(|x| *x == Field::Instructions).unwrap_or(0);
                }
                return None;
            }
            // The bot's own agent again, while it runs on a fallback.
            (KeyCode::Char('B'), _) if self.bots.get(&bot).is_some_and(|b| b.agent_step.is_some()) => {
                self.call("switchBack", json!({"botId": bot}), After::Nothing);
            }
            // Use the computer, on the Details page above the routines.
            (KeyCode::Char('u'), _) if !self.screen.is_null() => {
                let on = self.bots.get(&bot).is_some_and(|b| b.computer);
                self.call("updateBot", json!({"id": bot, "computer": !on}), After::Nothing);
            }
            (KeyCode::Down | KeyCode::Char('j'), _) => l.cursor = (l.cursor + 1).min(n.saturating_sub(1)),
            (KeyCode::Up | KeyCode::Char('k'), _) => l.cursor = l.cursor.saturating_sub(1),
            (KeyCode::Char('n'), _) => {
                self.overlays.push(Overlay::Routines(l));
                self.overlays.push(Overlay::Routine(Box::new(RoutineForm {
                    bot,
                    id: None,
                    name: Editor::default(),
                    instruction: Editor::default(),
                    when: Editor::default(),
                    original: Value::Null,
                    original_text: String::new(),
                    field: RoutineField::Name,
                    error: None,
                    saving: false,
                })));
                return None;
            }
            (KeyCode::Char('c'), _) => {
                // Or ask the bot to set one up: start the ask in its chat.
                let draft = self.drafts.entry(bot).or_default();
                if !draft.text.is_empty() {
                    draft.insert("\n");
                }
                draft.insert("I want a routine that ");
                self.focus = Focus::Chat;
                self.typing = true;
                return None;
            }
            (KeyCode::Enter | KeyCode::Char('e'), Some(r)) => {
                let triggers = r["triggers"].clone();
                let text = when_text(&triggers, &local_zone());
                let original_text = r["triggerDescriptions"]
                    .as_array()
                    .map(|d| d.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" · "))
                    .unwrap_or_default();
                self.overlays.push(Overlay::Routines(l));
                self.overlays.push(Overlay::Routine(Box::new(RoutineForm {
                    bot,
                    id: Some(s(&r, "id")),
                    name: Editor::with(&s(&r, "name")),
                    instruction: Editor::with(&s(&r, "instruction")),
                    when: Editor::with(text.as_deref().unwrap_or_default()),
                    original: if text.is_some() { Value::Null } else { triggers },
                    original_text,
                    field: RoutineField::Name,
                    error: None,
                    saving: false,
                })));
                return None;
            }
            (KeyCode::Char(' '), Some(r)) => {
                let on = !r["enabled"].as_bool().unwrap_or(false);
                self.sheet(
                    "setRoutineEnabled",
                    json!({"botId": bot, "id": r["id"], "enabled": on}),
                    Reply::Routines(bot.clone()),
                );
                if let Some(item) = l.items.as_mut().and_then(|v| v.get_mut(l.cursor)) {
                    item["enabled"] = on.into();
                }
            }
            (KeyCode::Char('r'), Some(r)) => {
                self.sheet("runRoutine", json!({"botId": bot, "id": r["id"]}), Reply::Routines(bot.clone()));
                self.flash(&format!("Running {} now", s(&r, "name")));
            }
            (KeyCode::Char('w' | 'W'), Some(r)) if !has_webhook(&r) => {
                self.flash("Only a routine that runs on a webhook has a URL and key");
            }
            (KeyCode::Char('w'), Some(r)) => {
                self.sheet("routineWebhook", json!({"botId": bot, "id": r["id"]}), Reply::Webhook);
            }
            (KeyCode::Char('W'), Some(r)) => {
                let c = Confirm {
                    title: format!("Replace the webhook key of {}?", s(&r, "name")),
                    detail: "Senders using the current key stop working.".into(),
                    note: "The new one is copied in a curl command.".into(),
                    button: "Replace",
                    act: ConfirmAct::RotateRoutineKey(bot, s(&r, "id")),
                };
                self.overlays.push(Overlay::Routines(l));
                self.overlays.push(Overlay::Confirm(c));
                return None;
            }
            (KeyCode::Char('x'), Some(r)) => {
                let c = Confirm {
                    title: format!("Delete the routine {}?", s(&r, "name")),
                    detail: "It stops running.".into(),
                    note: "What it already posted stays in the chat.".into(),
                    button: "Delete",
                    act: ConfirmAct::DeleteRoutine(bot, s(&r, "id")),
                };
                self.overlays.push(Overlay::Routines(l));
                self.overlays.push(Overlay::Confirm(c));
                return None;
            }
            _ => {}
        }
        Some(Overlay::Routines(l))
    }

    pub(in crate::tui) fn routine_key(&mut self, mut f: Box<RoutineForm>, k: KeyEvent) -> Option<Overlay> {
        use RoutineField::{Instruction, Name, When};
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
            _ if newline(k) && f.field == Instruction => f.instruction.insert("\n"),
            KeyCode::Enter => self.save_routine(&mut f),
            KeyCode::Char('s') if ctrl(k) => self.save_routine(&mut f),
            KeyCode::Tab | KeyCode::Down => {
                f.field = match f.field {
                    Name => Instruction,
                    Instruction => When,
                    When => Name,
                };
            }
            KeyCode::BackTab | KeyCode::Up => {
                f.field = match f.field {
                    Name => When,
                    Instruction => Name,
                    When => Instruction,
                };
            }
            _ => {
                let ed = match f.field {
                    Name => &mut f.name,
                    Instruction => &mut f.instruction,
                    When => &mut f.when,
                };
                ed.key(k);
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
        let mut body = json!({"botId": f.bot, "name": name, "instruction": instruction, "triggers": triggers});
        if let Some(id) = &f.id {
            body["id"] = id.clone().into();
        }
        f.saving = true;
        f.error = None;
        self.sheet("saveRoutine", body, Reply::RoutineSaved);
    }

    /// `F`: runs a failed routine run's work again: the picked notice's, else the newest in this chat.
    pub(in crate::tui) fn retry_failed_run(&mut self) {
        let Some(bot) = self.selected.clone() else { return };
        let run = self
            .lane(&bot)
            .into_iter()
            .filter(|e| e.retries() && self.pick.as_ref().is_none_or(|id| *id == e.id))
            .max_by_key(|e| e.seq)
            .map(|e| e.data["runId"].clone());
        match run {
            Some(run) => {
                self.call("retryRoutineRun", json!({"botId": bot, "runId": run}), After::Nothing);
                self.flash("Trying it again");
            }
            None => self.flash("No failed routine run to try again here"),
        }
    }
}

/// A routine with a webhook (or event) trigger, which has a URL and key.
fn has_webhook(r: &Value) -> bool {
    r["triggers"].as_array().into_iter().flatten().any(|t| matches!(t["type"].as_str(), Some("webhook" | "event")))
}
