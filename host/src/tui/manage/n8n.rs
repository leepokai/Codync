//! The computer's n8n in the terminal: whether it runs, Start and Stop, its workflows with a switch to
//! publish each, and its latest runs. Same words and the same host calls as the phone's section; the
//! host does the work.

use super::super::app::{App, Overlay};
use super::Reply;
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

/// Workflows listed at most; the rest are in n8n.
pub const SHOWN_WORKFLOWS: usize = 25;

#[derive(Clone)]
pub struct N8nReply(pub String, pub u64, pub N8nResult);

#[derive(Clone, Copy)]
pub enum N8nResult {
    Status,
    Workflows,
    Runs,
    /// A start, stop or switch went through: look again.
    Changed,
}

pub struct N8nSheet {
    pub scope: String,
    pub status: Value,
    pub workflows: Vec<Value>,
    pub more: bool,
    pub runs: Vec<Value>,
    pub cursor: usize,
    pub busy: bool,
    pub error: Option<String>,
    request: u64,
}

fn s<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

impl N8nSheet {
    pub fn running(&self) -> bool {
        s(&self.status, "state") == "running"
    }

    /// Starting or running: what Stop applies to.
    pub fn active(&self) -> bool {
        matches!(s(&self.status, "state"), "running" | "starting")
    }

    pub fn loaded(&self) -> bool {
        self.status.is_object()
    }

    /// The state as a catalog key (`{name}` is n8n).
    pub fn state_label(&self) -> &'static str {
        match s(&self.status, "state") {
            "running" => "Running",
            "starting" => "Starting {name}… the first start can take a few minutes.",
            "error" => "Stopped with an error",
            _ => "Stopped",
        }
    }
}

/// A run's status as a catalog key; the phone's words.
pub fn run_label(status: &str) -> &'static str {
    match status {
        "success" => "Succeeded",
        "error" => "Failed",
        "crashed" => "Crashed",
        "running" => "Running",
        "waiting" => "Waiting",
        "canceled" => "Cancelled",
        "new" => "Waiting in the queue",
        _ => "Unknown",
    }
}

impl App {
    pub fn open_n8n(&mut self) {
        let mut sheet = Box::new(N8nSheet {
            scope: uuid::Uuid::new_v4().to_string(),
            status: Value::Null,
            workflows: vec![],
            more: false,
            runs: vec![],
            cursor: 0,
            busy: false,
            error: None,
            request: 0,
        });
        self.n8n_request(&mut sheet, "n8nStatus", json!({}), N8nResult::Status);
        self.overlays.push(Overlay::N8n(sheet));
    }

    fn n8n_request(&mut self, sheet: &mut N8nSheet, method: &'static str, body: Value, kind: N8nResult) {
        sheet.request += 1;
        sheet.busy = true;
        sheet.error = None;
        self.sheet(method, body, Reply::N8n(N8nReply(sheet.scope.clone(), sheet.request, kind)));
    }

    /// Asks again for everything the sheet shows.
    fn n8n_refresh(&mut self, sheet: &mut N8nSheet) {
        self.n8n_request(sheet, "n8nStatus", json!({}), N8nResult::Status);
    }

    pub fn n8n_reply(&mut self, reply: &N8nReply, response: Result<Value, String>) {
        let Some(index) = self
            .overlays
            .iter()
            .position(|o| matches!(o, Overlay::N8n(sheet) if sheet.scope == reply.0 && sheet.request == reply.1))
        else {
            return;
        };
        let Overlay::N8n(mut sheet) = self.overlays.remove(index) else { return };
        sheet.busy = false;
        match response {
            Err(error) => sheet.error = Some(error),
            Ok(value) => match reply.2 {
                N8nResult::Status => {
                    sheet.status = value;
                    // Workflows and runs exist only while n8n answers.
                    if sheet.running() {
                        self.n8n_request(&mut sheet, "n8nWorkflows", json!({}), N8nResult::Workflows);
                    } else {
                        sheet.workflows.clear();
                        sheet.runs.clear();
                        sheet.more = false;
                    }
                }
                N8nResult::Workflows => {
                    sheet.workflows = value["workflows"].as_array().cloned().unwrap_or_default();
                    sheet.more = value["more"] == true;
                    sheet.cursor = sheet.cursor.min(sheet.workflows.len().saturating_sub(1));
                    self.n8n_request(&mut sheet, "n8nExecutions", json!({}), N8nResult::Runs);
                }
                N8nResult::Runs => {
                    sheet.runs = value["executions"].as_array().cloned().unwrap_or_default();
                }
                N8nResult::Changed => self.n8n_refresh(&mut sheet),
            },
        }
        self.overlays.insert(index, Overlay::N8n(sheet));
    }

    pub fn n8n_key(&mut self, mut sheet: Box<N8nSheet>, key: KeyEvent) -> Option<Overlay> {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return None,
            _ if sheet.busy => {}
            KeyCode::Char('r') => self.n8n_refresh(&mut sheet),
            KeyCode::Char('s') if sheet.loaded() => {
                let method = if sheet.active() { "n8nStop" } else { "n8nStart" };
                self.n8n_request(&mut sheet, method, json!({}), N8nResult::Changed);
            }
            KeyCode::Up | KeyCode::Char('k') => sheet.cursor = sheet.cursor.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                let last = sheet.workflows.len().min(SHOWN_WORKFLOWS).saturating_sub(1);
                sheet.cursor = (sheet.cursor + 1).min(last);
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                if let Some(workflow) = sheet.workflows.get(sheet.cursor) {
                    let body = json!({"id": s(workflow, "id"), "active": workflow["active"] != true});
                    self.n8n_request(&mut sheet, "n8nSetWorkflowActive", body, N8nResult::Changed);
                }
            }
            _ => {}
        }
        Some(Overlay::N8n(sheet))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(status: Value) -> N8nSheet {
        N8nSheet {
            scope: String::new(),
            status,
            workflows: vec![],
            more: false,
            runs: vec![],
            cursor: 0,
            busy: false,
            error: None,
            request: 0,
        }
    }

    #[test]
    fn the_state_reads_as_the_phone_and_desktop_do() {
        assert_eq!(sheet(Value::Null).state_label(), "Stopped");
        assert!(!sheet(Value::Null).loaded(), "no answer yet");
        let running = sheet(json!({"state": "running"}));
        assert!(running.running() && running.active());
        assert_eq!(running.state_label(), "Running");
        let starting = sheet(json!({"state": "starting"}));
        assert!(starting.active() && !starting.running());
        assert!(starting.state_label().starts_with("Starting {name}"));
        assert_eq!(sheet(json!({"state": "error"})).state_label(), "Stopped with an error");
        assert!(!sheet(json!({"state": "stopped"})).active());
    }

    #[test]
    fn a_runs_status_has_the_phones_words() {
        assert_eq!(run_label("success"), "Succeeded");
        assert_eq!(run_label("error"), "Failed");
        assert_eq!(run_label("crashed"), "Crashed");
        assert_eq!(run_label("new"), "Waiting in the queue");
        assert_eq!(run_label("something-new"), "Unknown");
    }
}
