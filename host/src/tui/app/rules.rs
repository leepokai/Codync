//! A bot's approval rules in the terminal: a sheet to add, edit, reorder and delete them, and to try a command against
//! them. The host decides what a rule does (`testApprovalRule`); this only words its answer.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use super::{After, App, Editor, Overlay};

/// The tool kinds a rule can be limited to.
pub const KINDS: [&str; 9] = ["execute", "edit", "delete", "move", "read", "search", "fetch", "think", "other"];
const DECISIONS: [&str; 3] = ["allow", "ask", "deny"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleRow {
    pub pattern: String,
    pub kind: Option<String>,
    pub decision: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    List,
    /// Typing the selected rule's pattern.
    Pattern,
    /// Typing a command to try.
    Test,
}

/// The quick rules approvals enforce: wire class and label (the host's table, `approval_rules::QUICK_KINDS`).
pub const QUICK_CLASSES: [(&str, &str); 3] = [("files", "files"), ("shell", "shell"), ("browse", "web")];

pub struct RulesSheet {
    pub bot_id: String,
    pub bot_name: String,
    pub rules: Vec<RuleRow>,
    pub sel: usize,
    pub mode: Mode,
    pub editor: Editor,
    /// What the host said about the last command tried.
    pub answer: Option<String>,
    pub saving: bool,
    pub error: Option<String>,
    /// Whether the quick rules are enforced; `None` against a host that has none.
    pub quick_on: Option<bool>,
    /// Every class's policy as the bot has it; only the three in [`QUICK_CLASSES`] are edited here.
    pub policies: serde_json::Map<String, Value>,
    /// The security profile the bot follows (`None`: its own rules), and whether `P` changed it in this sheet.
    pub profile: Option<String>,
    pub profile_changed: bool,
}

impl RulesSheet {
    pub fn new(bot_id: &str, bot_name: &str, saved: &[Value]) -> Self {
        let rules = saved
            .iter()
            .filter_map(|r| {
                Some(RuleRow {
                    pattern: r["pattern"].as_str()?.to_owned(),
                    kind: r["kind"].as_str().filter(|k| !k.is_empty()).map(str::to_owned),
                    decision: r["decision"].as_str().unwrap_or("ask").to_owned(),
                })
            })
            .collect();
        Self {
            bot_id: bot_id.to_owned(),
            bot_name: bot_name.to_owned(),
            rules,
            sel: 0,
            mode: Mode::List,
            editor: Editor::with(""),
            answer: None,
            saving: false,
            error: None,
            quick_on: None,
            policies: serde_json::Map::new(),
            profile: None,
            profile_changed: false,
        }
    }

    /// The same sheet with the bot's security profile.
    pub fn with_profile(mut self, profile: Option<String>) -> Self {
        self.profile = profile;
        self
    }

    /// What saving sends: the profile when `P` picked one (the host replaces the rules and the Approvals setting with
    /// its own), else the rules as edited.
    pub fn save_body(&self) -> Value {
        let mut body = json!({"id": self.bot_id});
        match (&self.profile, self.profile_changed) {
            (Some(profile), true) => body["securityProfile"] = profile.clone().into(),
            _ => body["approvalRules"] = self.body(),
        }
        if let Some(on) = self.quick_on {
            body["actionRulesOn"] = on.into();
            body["actionRules"] = Value::Object(self.policies.clone());
        }
        body
    }

    /// The same sheet with the bot's quick rules.
    pub fn with_quick(mut self, on: Option<bool>, policies: serde_json::Map<String, Value>) -> Self {
        self.quick_on = on;
        self.policies = policies;
        self
    }

    /// Class `at` (0 to 2 of [`QUICK_CLASSES`]) goes to its next policy: not set, allow, ask, block, not set.
    pub fn cycle_quick(&mut self, at: usize) {
        let Some((class, _)) = QUICK_CLASSES.get(at) else { return };
        let next = match self.policies.get(*class).and_then(Value::as_str) {
            None => Some("allow"),
            Some("allow") => Some("ask"),
            Some("ask") => Some("block"),
            _ => None,
        };
        match next {
            Some(policy) => self.policies.insert((*class).to_owned(), policy.into()),
            None => self.policies.remove(*class),
        };
    }

    /// The rules as the host reads them: blank patterns dropped, the rest trimmed.
    pub fn body(&self) -> Value {
        let rules: Vec<Value> = self
            .rules
            .iter()
            .filter(|r| !r.pattern.trim().is_empty())
            .map(|r| json!({"pattern": r.pattern.trim(), "kind": r.kind, "decision": r.decision}))
            .collect();
        Value::Array(rules)
    }

    pub fn cycle_decision(&mut self) {
        if let Some(rule) = self.rules.get_mut(self.sel) {
            let at = DECISIONS.iter().position(|d| *d == rule.decision).unwrap_or(1);
            DECISIONS[(at + 1) % DECISIONS.len()].clone_into(&mut rule.decision);
        }
    }

    /// Any tool, then each kind in turn, then any tool again.
    pub fn cycle_kind(&mut self) {
        if let Some(rule) = self.rules.get_mut(self.sel) {
            rule.kind = match rule.kind.as_deref().and_then(|k| KINDS.iter().position(|x| *x == k)) {
                None if rule.kind.is_none() => Some(KINDS[0].to_owned()),
                Some(i) if i + 1 < KINDS.len() => Some(KINDS[i + 1].to_owned()),
                _ => None,
            };
        }
    }

    /// Moves the selected rule by `delta` places (the first match decides, so order matters).
    pub fn move_selected(&mut self, delta: isize) {
        let to = self.sel.checked_add_signed(delta).filter(|to| *to < self.rules.len());
        if let Some(to) = to {
            self.rules.swap(self.sel, to);
            self.sel = to;
        }
    }
}

/// One sentence for the host's answer (`evaluation`); rule numbers count from 1, as the list shows them.
pub fn describe(e: &Value) -> String {
    let passed = e["allowPassedOver"]
        .as_u64()
        .map(|i| format!(" Rule {} would allow it, but it runs several commands.", i + 1))
        .unwrap_or_default();
    match e["outcome"].as_str() {
        Some("forbidden") => format!(
            "Never allowed: it matches \"{}\" in the list of forbidden commands, whatever the rules say.",
            e["pattern"].as_str().unwrap_or_default()
        ),
        Some("rule") => {
            let verb = match e["decision"].as_str() {
                Some("allow") => "Allowed",
                Some("deny") => "Denied",
                _ => "Asks you",
            };
            format!(
                "{verb} by rule {} (\"{}\").",
                e["index"].as_u64().unwrap_or(0) + 1,
                e["pattern"].as_str().unwrap_or_default()
            )
        }
        _ => {
            let how = if e["permission"] == "auto" { "it is approved automatically" } else { "it asks you" };
            format!("No rule matches: {how}, as the bot's Approvals setting says.{passed}")
        }
    }
}

impl App {
    /// Opens the selected bot's rules.
    pub(super) fn open_rules(&mut self) {
        let Some(bot) = self.bot().filter(|b| !b.group) else {
            self.flash("Pick a bot first");
            return;
        };
        let sheet = RulesSheet::new(&bot.id, &bot.name, &bot.approval_rules)
            .with_quick(bot.action_rules_on, bot.action_rules.clone())
            .with_profile(bot.security_profile.clone());
        if self.profiles.is_empty() {
            self.call("securityProfiles", json!({}), After::Profiles);
        }
        self.overlays.push(Overlay::Rules(Box::new(sheet)));
    }

    pub(super) fn approval_rules_key(&mut self, mut s: Box<RulesSheet>, k: KeyEvent) -> Option<Overlay> {
        match s.mode {
            Mode::Pattern => match k.code {
                KeyCode::Enter => {
                    let text = s.editor.text.trim().to_owned();
                    if text.is_empty() {
                        if s.sel < s.rules.len() {
                            s.rules.remove(s.sel);
                            s.sel = s.sel.min(s.rules.len().saturating_sub(1));
                        }
                    } else if let Some(rule) = s.rules.get_mut(s.sel) {
                        rule.pattern = text;
                    }
                    s.mode = Mode::List;
                }
                KeyCode::Esc => {
                    // A rule added and never given a pattern is no rule.
                    if s.rules.get(s.sel).is_some_and(|r| r.pattern.trim().is_empty()) {
                        s.rules.remove(s.sel);
                        s.sel = s.sel.min(s.rules.len().saturating_sub(1));
                    }
                    s.mode = Mode::List;
                }
                _ => {
                    s.editor.key(k);
                }
            },
            Mode::Test => match k.code {
                KeyCode::Enter if !s.editor.text.trim().is_empty() => {
                    let mut body = json!({"botId": s.bot_id, "command": s.editor.text.trim(), "rules": s.body()});
                    if let Some(on) = s.quick_on {
                        body["actionRulesOn"] = on.into();
                        body["actionRules"] = Value::Object(s.policies.clone());
                    }
                    self.call("testApprovalRule", body, After::RuleTest);
                }
                KeyCode::Esc => s.mode = Mode::List,
                _ => {
                    s.editor.key(k);
                    s.answer = None;
                }
            },
            Mode::List => match k.code {
                KeyCode::Esc | KeyCode::Char('q') => return None,
                KeyCode::Char('s') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                    s.saving = true;
                    s.error = None;
                    self.call("updateBot", s.save_body(), After::RulesSaved);
                }
                KeyCode::Down | KeyCode::Char('j') => s.sel = (s.sel + 1).min(s.rules.len().saturating_sub(1)),
                KeyCode::Up | KeyCode::Char('k') => s.sel = s.sel.saturating_sub(1),
                KeyCode::Char('a') => {
                    s.rules.push(RuleRow { pattern: String::new(), kind: None, decision: "ask".into() });
                    s.sel = s.rules.len() - 1;
                    s.editor = Editor::with("");
                    s.mode = Mode::Pattern;
                }
                KeyCode::Enter | KeyCode::Char('e') if !s.rules.is_empty() => {
                    s.editor = Editor::with(&s.rules[s.sel].pattern);
                    s.mode = Mode::Pattern;
                }
                KeyCode::Char('d' | 'x') if !s.rules.is_empty() => {
                    s.rules.remove(s.sel);
                    s.sel = s.sel.min(s.rules.len().saturating_sub(1));
                }
                KeyCode::Char('Q') => s.quick_on = s.quick_on.map(|on| !on),
                KeyCode::Char('P') => self.next_sheet_profile(&mut s),
                KeyCode::Char(digit @ '1'..='3') if s.quick_on.is_some() => {
                    s.cycle_quick(usize::from(digit as u8 - b'1'));
                }
                KeyCode::Char('K') => s.move_selected(-1),
                KeyCode::Char('J') => s.move_selected(1),
                KeyCode::Char('m') => s.cycle_decision(),
                KeyCode::Char('t') => s.cycle_kind(),
                KeyCode::Char('T' | '/') => {
                    s.editor = Editor::with("");
                    s.mode = Mode::Test;
                }
                _ => {}
            },
        }
        Some(Overlay::Rules(s))
    }

    /// `P`: the next security profile replaces the sheet's rules with that profile's, until it is saved or the sheet
    /// is closed.
    fn next_sheet_profile(&self, s: &mut RulesSheet) {
        let ids: Vec<&str> = self.profiles.iter().filter_map(|p| p["id"].as_str()).collect();
        if ids.is_empty() {
            return;
        }
        let at = s.profile.as_deref().and_then(|id| ids.iter().position(|x| *x == id));
        let next = ids[at.map_or(0, |i| (i + 1) % ids.len())];
        let Some(profile) = self.profiles.iter().find(|p| p["id"] == next) else { return };
        s.rules = profile["rules"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|r| RuleRow {
                pattern: "*".into(),
                kind: r["kind"].as_str().map(str::to_owned),
                decision: r["decision"].as_str().unwrap_or("ask").to_owned(),
            })
            .collect();
        s.sel = 0;
        s.profile = Some(next.to_owned());
        s.profile_changed = true;
    }

    /// The host's answer to a command tried in the sheet.
    pub(super) fn rules_tested(&mut self, v: &Value) {
        if let Some(Overlay::Rules(s)) = self.overlays.last_mut() {
            s.answer = Some(describe(&v["evaluation"]));
        }
    }

    /// The rules were saved (or refused): close the sheet, or say why not.
    pub(super) fn rules_saved(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                if matches!(self.overlays.last(), Some(Overlay::Rules(_))) {
                    self.overlays.pop();
                }
                self.flash("Approval rules saved");
            }
            Err(error) => {
                if let Some(Overlay::Rules(s)) = self.overlays.last_mut() {
                    s.saving = false;
                    s.error = Some(error);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> RulesSheet {
        RulesSheet::new(
            "b",
            "Bot",
            &[
                json!({"pattern": "git status*", "decision": "allow"}),
                json!({"pattern": "rm *", "kind": "execute", "decision": "deny"}),
            ],
        )
    }

    #[test]
    fn rules_load_move_and_cycle() {
        let mut s = sheet();
        assert_eq!(s.rules.len(), 2);
        s.move_selected(1);
        assert_eq!((s.rules[0].pattern.as_str(), s.sel), ("rm *", 1));
        s.move_selected(1);
        assert_eq!(s.sel, 1, "can't go past the end");
        s.cycle_decision();
        assert_eq!(s.rules[1].decision, "ask", "allow, then ask");
        s.sel = 0;
        s.cycle_kind();
        assert_eq!(s.rules[0].kind.as_deref(), Some("edit"), "execute, then edit");
        for _ in 1..KINDS.len() {
            s.cycle_kind();
        }
        assert_eq!(s.rules[0].kind, None, "after the last kind, any tool again");
    }

    #[test]
    fn quick_rules_cycle_through_their_policies_and_leave_other_classes_alone() {
        let mut s = sheet().with_quick(Some(false), serde_json::Map::from_iter([("bots".to_owned(), json!("block"))]));
        for expected in [Some("allow"), Some("ask"), Some("block"), None] {
            s.cycle_quick(1);
            assert_eq!(s.policies.get("shell").and_then(Value::as_str), expected);
        }
        s.cycle_quick(2);
        assert_eq!(s.policies.get("browse"), Some(&json!("allow")));
        assert_eq!(s.policies.get("bots"), Some(&json!("block")), "a class the sheet doesn't edit is kept");
        s.cycle_quick(9);
    }

    #[test]
    fn saving_sends_the_profile_when_one_was_picked_else_the_rules() {
        let mut s = sheet().with_profile(Some("builder".into()));
        assert!(s.save_body().get("approvalRules").is_some() && s.save_body().get("securityProfile").is_none());
        s.profile_changed = true;
        let body = s.save_body();
        assert_eq!(body["securityProfile"], "builder");
        assert!(body.get("approvalRules").is_none(), "the host replaces the rules with the profile's");
    }

    #[test]
    fn the_body_drops_blank_patterns_and_trims() {
        let mut s = sheet();
        s.rules.push(RuleRow { pattern: "  ".into(), kind: None, decision: "ask".into() });
        s.rules[0].pattern = "  ls  ".into();
        let body = s.body();
        assert_eq!(body.as_array().map(Vec::len), Some(2));
        assert_eq!(body[0]["pattern"], "ls");
        assert_eq!(body[1]["kind"], "execute");
    }

    #[test]
    fn the_hosts_answer_is_worded() {
        let forbidden = json!({"outcome": "forbidden", "pattern": "rm -rf /"});
        assert!(describe(&forbidden).starts_with("Never allowed"));
        let rule = json!({"outcome": "rule", "index": 1, "decision": "allow", "pattern": "git *"});
        assert_eq!(describe(&rule), "Allowed by rule 2 (\"git *\").");
        let setting = json!({"outcome": "setting", "permission": "ask", "allowPassedOver": 0});
        let text = describe(&setting);
        assert!(text.contains("it asks you") && text.contains("Rule 1 would allow it"), "{text}");
    }
}
