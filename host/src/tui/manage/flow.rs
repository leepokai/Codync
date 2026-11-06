//! A routine's flow in the terminal: the chain routine → bot → bot with the status of the latest run's
//! steps, and keys to connect, change, disconnect and test it. No canvas: the host's canvas methods
//! (`routineFlowCanvas` and the rest) are what the graphical apps draw.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, Editor, Overlay};
use super::{Reply, RoutineList, s};

/// Deepest chain the sheet draws; the host refuses longer flows anyway.
const MAX_ROWS_DEPTH: usize = 16;
/// Rows drawn at most, whatever the links.
const MAX_ROWS: usize = 200;

pub struct FlowSheet {
    /// The bot that owns the routine.
    pub bot: String,
    pub routine: String,
    pub name: String,
    /// The owner's canvas, as `routineFlowCanvas` answered; `None` while it loads.
    pub canvas: Option<Value>,
    pub cursor: usize,
    pub stage: FlowStage,
}

pub enum FlowStage {
    Browse,
    /// Choosing the bot that gets the reply of the row's bot.
    Pick {
        cursor: usize,
    },
    /// Typing what that bot does with it (or the new instruction of a link).
    Instruction {
        target: Target,
        text: Editor,
    },
}

pub enum Target {
    New { from: String, to: String },
    Link(String),
}

/// One line of the chain: a bot, how deep, the link that brought the reply to it and its step.
pub struct FlowRow {
    pub bot: String,
    pub depth: usize,
    pub link: Option<Value>,
    pub step: Option<Value>,
}

/// The chain as lines: the routine's bot, then each link's bot under the one that hands to it. A bot
/// reached by two links appears under each.
pub fn rows(canvas: &Value, routine: &str, owner: &str) -> Vec<FlowRow> {
    let links: Vec<&Value> =
        canvas["links"].as_array().into_iter().flatten().filter(|l| l["routineId"] == routine).collect();
    let steps = canvas["routines"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|r| r["routine"]["id"] == routine)
        .and_then(|r| r["steps"].as_array());
    let mut out = vec![];
    walk(&links, steps.map_or(&[], Vec::as_slice), (owner, 0, None), &mut out);
    out
}

fn walk(links: &[&Value], steps: &[Value], (bot, depth, link): (&str, usize, Option<&Value>), out: &mut Vec<FlowRow>) {
    if out.len() >= MAX_ROWS {
        return;
    }
    let step = steps.iter().find(|st| st["botId"] == bot).cloned();
    out.push(FlowRow { bot: bot.to_owned(), depth, link: link.cloned(), step });
    if depth < MAX_ROWS_DEPTH {
        for next in links.iter().filter(|l| l["fromBotId"] == bot) {
            walk(links, steps, (&s(next, "toBotId"), depth + 1, Some(next)), out);
        }
    }
}

impl FlowSheet {
    pub fn rows(&self) -> Vec<FlowRow> {
        self.canvas.as_ref().map_or_else(Vec::new, |c| rows(c, &self.routine, &self.bot))
    }
}

impl App {
    pub(in crate::tui) fn open_flow(&mut self, list: RoutineList, routine: &Value) {
        let (bot, id) = (list.bot.clone(), s(routine, "id"));
        self.sheet("routineFlowCanvas", json!({"botId": bot}), Reply::Flow(bot.clone()));
        let sheet =
            FlowSheet { bot, routine: id, name: s(routine, "name"), canvas: None, cursor: 0, stage: FlowStage::Browse };
        self.overlays.push(Overlay::Routines(list));
        self.overlays.push(Overlay::Flow(Box::new(sheet)));
    }

    /// The bots a reply can go to from `from`: agents of their own that aren't linked from it yet.
    pub(in crate::tui) fn flow_candidates(&self, sheet: &FlowSheet, from: &str) -> Vec<(String, String)> {
        let linked: Vec<String> = sheet
            .canvas
            .as_ref()
            .and_then(|c| c["links"].as_array())
            .into_iter()
            .flatten()
            .filter(|l| l["routineId"] == sheet.routine.as_str() && l["fromBotId"] == from)
            .map(|l| s(l, "toBotId"))
            .collect();
        let mut all: Vec<(String, String)> = self
            .bots
            .values()
            .filter(|b| !b.group && !b.hidden && b.backend != "external")
            .filter(|b| b.id != sheet.bot && b.id != from && !linked.contains(&b.id))
            .map(|b| (b.id.clone(), b.name.clone()))
            .collect();
        all.sort_by(|a, b| a.1.cmp(&b.1));
        all
    }

    pub(in crate::tui) fn flow_key(&mut self, mut f: Box<FlowSheet>, k: KeyEvent) -> Option<Overlay> {
        let rows = f.rows();
        let row = rows.get(f.cursor);
        let from = row.map_or_else(|| f.bot.clone(), |r| r.bot.clone());
        let link = row.and_then(|r| r.link.clone());
        f.stage = match std::mem::replace(&mut f.stage, FlowStage::Browse) {
            FlowStage::Browse => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return None,
                KeyCode::Down | KeyCode::Char('j') => {
                    f.cursor = (f.cursor + 1).min(rows.len().saturating_sub(1));
                    FlowStage::Browse
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    f.cursor = f.cursor.saturating_sub(1);
                    FlowStage::Browse
                }
                KeyCode::Char('a') if self.flow_candidates(&f, &from).is_empty() => {
                    self.flash("No other bot to hand the reply to");
                    FlowStage::Browse
                }
                KeyCode::Char('a') => FlowStage::Pick { cursor: 0 },
                KeyCode::Char('e') => {
                    if let Some(link) = link {
                        FlowStage::Instruction {
                            target: Target::Link(s(&link, "id")),
                            text: Editor::with(&s(&link, "instruction")),
                        }
                    } else {
                        self.flash("Pick a bot below the routine to change what it is asked");
                        FlowStage::Browse
                    }
                }
                KeyCode::Char('d' | 'x') => {
                    match link {
                        Some(link) => {
                            let body = json!({"linkId": link["id"]});
                            self.sheet("disconnectRoutineFlow", body, Reply::Flow(f.bot.clone()));
                        }
                        None => self.flash("The routine's own bot starts the flow; pick a bot below it to disconnect"),
                    }
                    FlowStage::Browse
                }
                KeyCode::Char('t') => {
                    let body = json!({"botId": f.bot, "routineId": f.routine});
                    self.sheet("testRoutineFlow", body, Reply::Flow(f.bot.clone()));
                    self.flash("Testing the flow with a sample reply");
                    FlowStage::Browse
                }
                _ => FlowStage::Browse,
            },
            FlowStage::Pick { cursor } => {
                let candidates = self.flow_candidates(&f, &from);
                match k.code {
                    KeyCode::Esc | KeyCode::Char('q') => FlowStage::Browse,
                    KeyCode::Down | KeyCode::Char('j') => {
                        FlowStage::Pick { cursor: (cursor + 1).min(candidates.len().saturating_sub(1)) }
                    }
                    KeyCode::Up | KeyCode::Char('k') => FlowStage::Pick { cursor: cursor.saturating_sub(1) },
                    KeyCode::Enter => match candidates.get(cursor) {
                        Some((to, _)) => FlowStage::Instruction {
                            target: Target::New { from, to: to.clone() },
                            text: Editor::default(),
                        },
                        None => FlowStage::Browse,
                    },
                    _ => FlowStage::Pick { cursor },
                }
            }
            FlowStage::Instruction { target, mut text } => match k.code {
                KeyCode::Esc => FlowStage::Browse,
                KeyCode::Enter => {
                    self.save_flow_link(&f, &target, text.text.trim());
                    FlowStage::Browse
                }
                _ => {
                    text.key(k);
                    FlowStage::Instruction { target, text }
                }
            },
        };
        Some(Overlay::Flow(f))
    }

    fn save_flow_link(&mut self, f: &FlowSheet, target: &Target, instruction: &str) {
        let reply = Reply::Flow(f.bot.clone());
        match target {
            Target::New { from, to } => {
                let body =
                    json!({"routineId": f.routine, "fromBotId": from, "toBotId": to, "instruction": instruction});
                self.sheet("connectRoutineFlow", body, reply);
            }
            Target::Link(id) => {
                self.sheet("updateRoutineFlowLink", json!({"linkId": id, "instruction": instruction}), reply);
            }
        }
    }

    /// The host changed a flow: an open sheet of that canvas loads it again.
    pub(in crate::tui) fn flow_changed(&mut self, bots: &Value) {
        let changed: Vec<&str> = bots.as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let open: Vec<String> = self
            .overlays
            .iter()
            .filter_map(|o| match o {
                Overlay::Flow(f) if changed.contains(&f.bot.as_str()) => Some(f.bot.clone()),
                _ => None,
            })
            .collect();
        for bot in open {
            self.sheet("routineFlowCanvas", json!({"botId": bot}), Reply::Flow(bot.clone()));
        }
    }

    /// An answer to a canvas request, or to a change that needs the canvas again.
    pub(in crate::tui) fn flow_loaded(&mut self, bot: &str, v: &Value) {
        if !v["routines"].is_array() {
            self.sheet("routineFlowCanvas", json!({"botId": bot}), Reply::Flow(bot.to_owned()));
            return;
        }
        for o in &mut self.overlays {
            if let Overlay::Flow(f) = o
                && f.bot == bot
            {
                f.cursor = f.cursor.min(rows(v, &f.routine, &f.bot).len().saturating_sub(1));
                f.canvas = Some(v.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chain_reads_top_down_with_the_latest_steps() {
        let canvas = json!({
            "links": [
                {"id": "1", "routineId": "r", "fromBotId": "a", "toBotId": "b", "instruction": "x"},
                {"id": "2", "routineId": "r", "fromBotId": "b", "toBotId": "c", "instruction": ""},
                {"id": "3", "routineId": "r", "fromBotId": "a", "toBotId": "d", "instruction": ""},
                {"id": "4", "routineId": "other", "fromBotId": "a", "toBotId": "z", "instruction": ""},
            ],
            "routines": [{"routine": {"id": "r"}, "steps": [{"botId": "b", "status": "failed"}]}],
        });
        let lines: Vec<(String, usize)> = rows(&canvas, "r", "a").into_iter().map(|r| (r.bot, r.depth)).collect();
        assert_eq!(lines, [("a".into(), 0), ("b".into(), 1), ("c".into(), 2), ("d".into(), 1)]);
        assert_eq!(rows(&canvas, "r", "a")[1].step.as_ref().unwrap()["status"], "failed");
    }

    #[test]
    fn a_loop_in_the_links_cannot_run_the_drawing_away() {
        let canvas = json!({"links": [
            {"id": "1", "routineId": "r", "fromBotId": "a", "toBotId": "b"},
            {"id": "2", "routineId": "r", "fromBotId": "b", "toBotId": "a"},
        ]});
        assert!(rows(&canvas, "r", "a").len() <= MAX_ROWS);
    }
}
