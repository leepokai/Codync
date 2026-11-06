//! The composer's `/` commands, listed while the draft is just `/partial`: Fast mode runs here
//! (`updateBot`); a skill fills in `/<id> `, which the host turns into "read this skill first"
//! (`market::expand_command`). A group offers the same, over its members.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::json;

use super::{After, App, Bot, Editor};

/// What picking a command does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashAction {
    Fast,
    Skill,
}

/// A command the list offers.
pub struct SlashCommand {
    /// What follows the `/`.
    pub name: String,
    pub title: String,
    pub detail: String,
    pub action: SlashAction,
}

impl SlashCommand {
    /// The row's text after `/name`.
    pub fn about(&self) -> String {
        match self.action {
            SlashAction::Skill if self.detail.is_empty() => self.title.clone(),
            SlashAction::Skill => self.detail.replace('\n', " "),
            SlashAction::Fast => format!("{} · {}", self.title, self.detail),
        }
    }
}

impl App {
    /// The bots the commands reach: the selected bot, or a group's members.
    fn reached(&self) -> Vec<&Bot> {
        let Some(bot) = self.bot() else { return vec![] };
        if bot.group { bot.members.iter().filter_map(|m| self.bots.get(m)).collect() } else { vec![bot] }
    }

    /// The bots `/fast` sets: those reached that offer Fast mode.
    fn fast_bots(&self) -> Vec<&Bot> {
        self.reached().into_iter().filter(|b| b.fast_shown()).collect()
    }

    /// Fast mode, when the bot (or any member of a group) offers it.
    fn built_in(&self) -> Vec<SlashCommand> {
        let bots = self.fast_bots();
        let detail = match bots.as_slice() {
            [] => return vec![],
            [bot] if self.bot().is_some_and(|b| !b.group) => bot.fast_detail(),
            _ => group_fast_detail(&bots),
        };
        vec![SlashCommand { name: "fast".into(), title: "Fast mode".into(), detail, action: SlashAction::Fast }]
    }

    /// Built-in commands, then the skills the bot (or any member of a group) has on, whose name
    /// or title holds the `/partial` being typed.
    pub fn slash_commands(&self) -> Vec<SlashCommand> {
        let draft = self.draft();
        let Some((query, None)) = crate::market::slash_command(&draft.text) else { return vec![] };
        let query = query.to_lowercase();
        let bots = self.reached();
        let skills = self.plugins.1.iter().filter_map(|s| {
            let id = s["id"].as_str()?;
            bots.iter().any(|b| b.skills.iter().any(|on| on == id)).then(|| SlashCommand {
                name: id.to_owned(),
                title: s["name"].as_str().unwrap_or(id).to_owned(),
                detail: s["description"].as_str().unwrap_or_default().to_owned(),
                action: SlashAction::Skill,
            })
        });
        self.built_in()
            .into_iter()
            .chain(skills)
            .filter(|c| c.name.contains(&query) || c.title.to_lowercase().contains(&query))
            .collect()
    }

    /// The highlighted row of `count`: kept while the draft stays the same, else the first.
    pub fn slash_selected(&self, count: usize) -> usize {
        let (draft, index) = &self.slash_pick;
        if *draft == self.draft().text { (*index).min(count.saturating_sub(1)) } else { 0 }
    }

    /// Keys while the list is up: ↑/↓ move, Tab or Return runs the highlighted command (Return
    /// sends a skill whose name is already typed out). Returns whether it took the key.
    pub(super) fn slash_key(&mut self, k: KeyEvent) -> bool {
        if !k.modifiers.is_empty() {
            return false;
        }
        let commands = self.slash_commands();
        let Some(n) = std::num::NonZeroUsize::new(commands.len()).map(usize::from) else { return false };
        let typed = self.draft().text;
        let sel = self.slash_selected(n);
        match k.code {
            KeyCode::Down => self.slash_pick = (typed, (sel + 1) % n),
            KeyCode::Up => self.slash_pick = (typed, (sel + n - 1) % n),
            KeyCode::Tab | KeyCode::Enter => {
                let picked = &commands[sel];
                if k.code == KeyCode::Enter && picked.action == SlashAction::Skill && typed[1..] == picked.name {
                    return false;
                }
                self.run_slash(picked);
            }
            _ => return false,
        }
        true
    }

    /// A draft that is just a built-in command (`/fast`) runs it instead of being sent.
    /// Returns whether it did.
    pub(super) fn built_in_command(&mut self, text: &str) -> bool {
        let Some(name) = text.strip_prefix('/') else { return false };
        let Some(command) = self.built_in().into_iter().find(|c| c.name == name) else { return false };
        self.run_slash(&command);
        true
    }

    fn run_slash(&mut self, command: &SlashCommand) {
        let fill = match command.action {
            // Each bot's own setting, so it holds in its own chat too.
            SlashAction::Fast => {
                let bots = self.fast_bots();
                let on = !bots.is_empty() && bots.iter().all(|b| b.fast_mode);
                for b in bots {
                    self.call("updateBot", json!({"id": b.id, "fastMode": !on}), After::Nothing);
                }
                self.flash(if on { "Fast mode off" } else { "Fast mode on" });
                String::new()
            }
            SlashAction::Skill => format!("/{} ", command.name),
        };
        if let Some(key) = self.selected.clone() {
            self.drafts.insert(key, Editor::with(&fill));
        }
    }

    /// Starting a draft with `/` loads the computer's skills again, so the menu is current.
    pub(super) fn slash_typed(&self) {
        if self.draft().text == "/" {
            self.call("skills", json!({}), After::Skills);
        }
    }
}

/// A group's Fast mode: on when every member offering it has it on; then the first member whose
/// agent refused it says why.
fn group_fast_detail(bots: &[&Bot]) -> String {
    if !bots.iter().all(|b| b.fast_mode) {
        return "Turn on for every bot here: faster replies and more usage".into();
    }
    bots.iter()
        .find_map(|b| b.fast_refusal().map(|why| format!("{}: {why}", b.name)))
        .unwrap_or_else(|| "Turn off for every bot here and return to standard speed".into())
}

#[cfg(test)]
mod tests {
    use super::super::{Bot, test_app};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use serde_json::json;

    fn app_with(bot: &serde_json::Value) -> super::App {
        let mut app = test_app();
        app.bots.insert("b".into(), Bot::parse(bot).expect("a bot parses"));
        app.selected = Some("b".into());
        app.plugins.1 = vec![
            json!({"id": "pdf", "name": "pdf", "description": "PDFs"}),
            json!({"id": "docx", "name": "docx"}),
            json!({"id": "xlsx", "name": "xlsx"}),
        ];
        app
    }

    fn names(app: &super::App) -> Vec<String> {
        app.slash_commands().into_iter().map(|c| c.name).collect()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[tokio::test]
    async fn the_list_offers_built_in_commands_then_the_bots_skills() {
        let mut app =
            app_with(&json!({"id": "b", "name": "Bot", "skills": ["pdf", "docx"], "fastModeSupport": "offered"}));
        app.drafts.insert("b".into(), super::Editor::with("/"));
        assert_eq!(names(&app), ["fast", "pdf", "docx"], "only skills the bot has on");
        app.drafts.insert("b".into(), super::Editor::with("/new"));
        assert!(names(&app).is_empty(), "no command for a new session");
        app.bots.get_mut("b").expect("the bot").fast_support = "unknown".into();
        app.drafts.insert("b".into(), super::Editor::with("/"));
        assert_eq!(names(&app), ["fast", "pdf", "docx"], "offered until the agent says otherwise");
        app.bots.get_mut("b").expect("the bot").fast_support = "unsupported".into();
        assert_eq!(names(&app), ["pdf", "docx"], "no Fast mode on offer");
    }

    #[tokio::test]
    async fn arrows_pick_a_row_and_return_runs_it() {
        let mut app = app_with(&json!({"id": "b", "name": "Bot", "skills": ["pdf"], "fastModeSupport": "offered"}));
        app.drafts.insert("b".into(), super::Editor::with("/"));
        assert!(app.slash_key(key(KeyCode::Up)), "up wraps to the last row");
        assert_eq!(app.slash_selected(2), 1);
        assert!(app.slash_key(key(KeyCode::Enter)), "Return on a skill fills it in");
        assert_eq!(app.draft().text, "/pdf ");
        assert!(names(&app).is_empty(), "the list closes once the name is done");
        app.drafts.insert("b".into(), super::Editor::with("/pdf"));
        assert!(!app.slash_key(key(KeyCode::Enter)), "a skill already typed out sends");
        app.drafts.insert("b".into(), super::Editor::with("/fa"));
        assert!(app.slash_key(key(KeyCode::Tab)), "a built-in command runs");
        assert_eq!(app.draft().text, "", "and the draft clears");
    }

    #[tokio::test]
    async fn a_group_offers_its_members_fast_mode_and_skills() {
        let mut app = app_with(&json!({"id": "b", "name": "Bea", "skills": ["docx"], "fastModeSupport": "offered"}));
        let members = [("c", "Cal", json!(["xlsx", "docx"])), ("d", "Dot", json!([]))];
        for (id, name, skills) in members {
            let bot = json!({"id": id, "name": name, "skills": skills, "fastModeSupport": "offered"});
            app.bots.insert(id.into(), Bot::parse(&bot).expect("a bot parses"));
        }
        let group = json!({"id": "g", "kind": "group", "members": ["b", "c", "d"]});
        app.bots.insert("g".into(), Bot::parse(&group).expect("a group parses"));
        app.selected = Some("g".into());
        app.drafts.insert("g".into(), super::Editor::with("/"));
        assert_eq!(names(&app), ["fast", "docx", "xlsx"], "every member's skills, once each");
        let detail = |app: &super::App| app.slash_commands().remove(0).detail;
        assert!(detail(&app).starts_with("Turn on for every bot here"));
        app.bots.get_mut("b").expect("Bea").fast_mode = true;
        assert!(detail(&app).starts_with("Turn on"), "on only when every member has it on");
        app.bots.get_mut("d").expect("Dot").fast_support = "unsupported".into();
        app.bots.get_mut("c").expect("Cal").fast_mode = true;
        assert!(detail(&app).starts_with("Turn off for every bot here"), "Dot has no Fast mode to count");
        let cal = app.bots.get_mut("c").expect("Cal");
        (cal.fast_support, cal.fast_issue) = ("refused".into(), Some("requires extra usage".into()));
        assert!(detail(&app).starts_with("Cal: The agent turned it off: requires extra usage"));
        assert!(app.built_in_command("/fast"), "a bare /fast runs in a group too");
        assert_eq!(app.hint.as_ref().map(|(text, _)| text.as_str()), Some("Fast mode off"));
        for id in ["b", "c"] {
            let bot = app.bots.get_mut(id).expect("a member");
            (bot.fast_mode, bot.fast_support) = (false, "unsupported".into());
        }
        app.drafts.insert("g".into(), super::Editor::with("/"));
        assert_eq!(names(&app), ["docx", "xlsx"], "no member offers Fast mode");
        assert!(!app.built_in_command("/fast"), "then it reaches the bots as typed");
    }

    #[tokio::test]
    async fn sending_just_a_built_in_command_runs_it() {
        let mut app = app_with(&json!({"id": "b", "name": "Bot", "fastModeSupport": "offered"}));
        assert!(!app.built_in_command("/fast please"), "only the bare command");
        assert!(!app.built_in_command("/new"), "reaches the agent as typed");
        assert!(app.built_in_command("/fast"));
        app.bots.get_mut("b").expect("the bot").fast_support = "unsupported".into();
        assert!(!app.built_in_command("/fast"), "without Fast mode it reaches the agent");
    }
}
