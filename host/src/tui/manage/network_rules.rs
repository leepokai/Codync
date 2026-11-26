//! A network's rules in the network sheet: who on its other computers may message this
//! computer's bots, the first match deciding (`codync_network::rules`).

use codync_network::Access;
use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, cycle};
use super::super::l10n::{fill, tr};
use super::network::{NetReply, NetworkSheet, Step};

const ANY: &str = "*";

/// The levels a rule can take, the most trusted first, as the form cycles them.
pub const LEVELS: [Access; 4] = [Access::Allow, Access::Auto, Access::Ask, Access::Deny];

/// How the form names a level.
pub fn level_name(access: Access) -> &'static str {
    match access {
        Access::Allow => "Allow",
        Access::Auto => "Auto",
        Access::Ask => "Ask",
        Access::Deny => "Deny",
    }
}

/// What a level lets its sender have a bot here do.
pub fn level_detail(access: Access) -> &'static str {
    match access {
        Access::Allow => "Its requests run as the bot is set.",
        Access::Auto => "The bot does by itself only what its agent judges safe, and asks you about the rest.",
        Access::Ask => "Every change and command the bot makes for it waits for your approval.",
        Access::Deny => "Its messages are refused.",
    }
}

/// A new rule: each choice cycles through "any" and the names the network offers. The picks are
/// ids, so a `network` event that reorders the choices under the form doesn't change them.
pub struct RuleForm {
    pub access: Access,
    pub from_computer: String,
    pub from_agent: String,
    pub to_agent: String,
    /// Put it on top instead of after the others.
    pub first: bool,
    pub field: RuleField,
}

impl Default for RuleForm {
    fn default() -> Self {
        Self {
            access: Access::Allow,
            from_computer: ANY.into(),
            from_agent: ANY.into(),
            to_agent: ANY.into(),
            first: false,
            field: RuleField::default(),
        }
    }
}

/// The levels a rule to `to_agent` can take: a coding agent's session's are the ones the host
/// sends with it (`levels`).
pub fn levels(n: &NetworkSheet, to_agent: &str) -> Vec<Access> {
    n.session_levels(to_agent).unwrap_or_else(|| LEVELS.to_vec())
}

/// How the form shows the pick `id` of `list`: its name, or the id itself once it's gone.
pub fn label(list: &[(String, String)], id: &str) -> String {
    list.iter().find(|c| c.0 == id).map_or_else(|| id.to_owned(), |c| c.1.clone())
}

/// The pick `by` steps from `id` in `list`.
fn step<T: PartialEq + Clone>(list: &[T], at: &T, by: isize) -> T {
    let i = list.iter().position(|x| x == at).unwrap_or(0);
    list[cycle(i, by, list.len())].clone()
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum RuleField {
    #[default]
    Access,
    FromComputer,
    FromAgent,
    ToAgent,
    First,
}

const FIELDS: [RuleField; 5] =
    [RuleField::Access, RuleField::FromComputer, RuleField::FromAgent, RuleField::ToAgent, RuleField::First];

/// What one of the rule form's choices offers: `*` first, then names and how they read.
pub struct Choices {
    pub computers: Vec<(String, String)>,
    pub agents: Vec<(String, String)>,
    pub mine: Vec<(String, String)>,
}

impl NetworkSheet {
    /// The other computers, the agents of the one picked, and this computer's own.
    pub fn rule_choices(&self, form: &RuleForm) -> Choices {
        let any = |label: &str| (ANY.to_owned(), tr(label).into_owned());
        let agents_of = |c: &Value| -> Vec<(String, String)> {
            c["agents"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|a| {
                    (s(a, "id"), a["name"].as_str().unwrap_or_else(|| a["id"].as_str().unwrap_or_default()).into())
                })
                .collect()
        };
        let others: Vec<&Value> = self.computers().iter().filter(|c| c["me"] != true).collect();
        let mut computers = vec![any("Any computer")];
        computers.extend(others.iter().map(|c| (s(c, "name"), s(c, "name"))));
        let mut agents = vec![any("Any bot")];
        if let Some(c) = others.iter().find(|c| c["name"] == form.from_computer.as_str()) {
            agents.extend(agents_of(c));
        }
        let mut mine = vec![any("Any bot here")];
        if let Some(me) = self.me() {
            mine.extend(agents_of(me).into_iter().filter(|(id, _)| self.session_levels(id).is_none()));
        }
        // This computer's coding agents' sessions, by address (a rule can only allow or deny them).
        mine.extend(self.sessions().iter().map(|session| (s(session, "address"), s(session, "address"))));
        Choices { computers, agents, mine }
    }

    /// This computer's coding agents' sessions, in the order the host lists them.
    pub fn sessions(&self) -> &[Value] {
        self.view.as_ref().and_then(|v| v["sessions"].as_array()).map_or(&[], Vec::as_slice)
    }

    /// The levels a rule to `address` can take, when it is one of this computer's coding agents'
    /// sessions.
    fn session_levels(&self, address: &str) -> Option<Vec<Access>> {
        let session = self.sessions().iter().find(|s| s["address"] == address)?;
        Some(serde_json::from_value(session["levels"].clone()).unwrap_or_else(|_| vec![Access::Allow, Access::Deny]))
    }

    /// How a rule reads: "Deny: any bot on laptop → any bot here".
    pub fn rule_text(&self, rule: &Value) -> String {
        let (computer, agent) = (s(rule, "fromComputer"), s(rule, "fromAgent"));
        let from = match (computer.as_str(), agent.as_str()) {
            (ANY, ANY) => tr("any bot on any computer").into_owned(),
            (ANY, agent) => tr("{bot} on any computer").replace("{bot}", agent),
            (computer, ANY) => tr("any bot on {computer}").replace("{computer}", computer),
            (computer, agent) => {
                let on = self
                    .computers()
                    .iter()
                    .find(|c| c["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(computer)));
                let name = on.and_then(|c| agent_name(c, agent)).unwrap_or(agent);
                fill(&tr("{bot} on {computer}"), &[("bot", name), ("computer", computer)])
            }
        };
        let to = match s(rule, "toAgent").as_str() {
            ANY => tr("any bot here").into_owned(),
            id => self.me().and_then(|me| agent_name(me, id)).unwrap_or(id).to_owned(),
        };
        // An access this build doesn't know reads as deny, never wider than it is.
        let template = match serde_json::from_value(rule["access"].clone()).unwrap_or(Access::Deny) {
            Access::Allow => "Allow: {from} → {to}",
            Access::Auto => "Auto: {from} → {to}",
            Access::Ask => "Ask: {from} → {to}",
            Access::Deny => "Deny: {from} → {to}",
        };
        fill(&tr(template), &[("from", &from), ("to", &to)])
    }
}

/// What one of `computer`'s agents is called, by its id.
fn agent_name<'a>(computer: &'a Value, id: &str) -> Option<&'a str> {
    computer["agents"].as_array()?.iter().find(|a| a["id"] == id)?["name"].as_str()
}

fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or_default().to_owned()
}

impl App {
    /// Keys over the rules list: `n` adds one, `x` deletes, `K`/`J` move the picked one.
    pub(super) fn rules_key(&mut self, n: &mut NetworkSheet, at: usize, k: KeyEvent) -> Step {
        let mut rules: Vec<Value> = n.rules().to_vec();
        let last = rules.len().saturating_sub(1);
        match k.code {
            KeyCode::Esc | KeyCode::Char('q' | 'w') => return Step::List,
            KeyCode::Down | KeyCode::Char('j') => return Step::Rules((at + 1).min(last)),
            KeyCode::Up | KeyCode::Char('k') => return Step::Rules(at.saturating_sub(1)),
            KeyCode::Char('n') => return Step::NewRule(RuleForm::default()),
            KeyCode::Char('x') if at < rules.len() => {
                rules.remove(at);
            }
            KeyCode::Char('K') if at > 0 && at < rules.len() => rules.swap(at, at - 1),
            KeyCode::Char('J') if at < last => rules.swap(at, at + 1),
            _ => return Step::Rules(at),
        }
        let moved = match k.code {
            KeyCode::Char('K') => at - 1,
            KeyCode::Char('J') => at + 1,
            _ => at.min(rules.len().saturating_sub(1)),
        };
        self.save_rules(n, &rules);
        Step::Rules(moved)
    }

    pub(super) fn rule_form_key(&mut self, n: &mut NetworkSheet, mut form: RuleForm, k: KeyEvent) -> Step {
        let choices = n.rule_choices(&form);
        let i = FIELDS.iter().position(|f| *f == form.field).unwrap_or(0);
        match k.code {
            KeyCode::Esc => return Step::Rules(0),
            KeyCode::Tab | KeyCode::Down => form.field = FIELDS[cycle(i, 1, FIELDS.len())],
            KeyCode::BackTab | KeyCode::Up => form.field = FIELDS[cycle(i, -1, FIELDS.len())],
            KeyCode::Left | KeyCode::Right | KeyCode::Char(' ') => {
                let by = if k.code == KeyCode::Left { -1 } else { 1 };
                let ids = |list: &[(String, String)]| list.iter().map(|c| c.0.clone()).collect::<Vec<_>>();
                match form.field {
                    RuleField::Access => form.access = step(&levels(n, &form.to_agent), &form.access, by),
                    RuleField::FromComputer => {
                        form.from_computer = step(&ids(&choices.computers), &form.from_computer, by);
                        form.from_agent = ANY.into();
                    }
                    RuleField::FromAgent => form.from_agent = step(&ids(&choices.agents), &form.from_agent, by),
                    RuleField::ToAgent => {
                        form.to_agent = step(&ids(&choices.mine), &form.to_agent, by);
                        if !levels(n, &form.to_agent).contains(&form.access) {
                            form.access = Access::Deny;
                        }
                    }
                    RuleField::First => form.first = !form.first,
                }
            }
            KeyCode::Enter => {
                let rule = json!({
                    "fromComputer": form.from_computer,
                    "fromAgent": form.from_agent,
                    "toAgent": form.to_agent,
                    "access": form.access,
                });
                let mut rules: Vec<Value> = n.rules().to_vec();
                if form.first {
                    rules.insert(0, rule);
                } else {
                    rules.push(rule);
                }
                self.save_rules(n, &rules);
                return Step::Rules(if form.first { 0 } else { n.rules().len() });
            }
            _ => {}
        }
        Step::NewRule(form)
    }

    fn save_rules(&self, n: &mut NetworkSheet, rules: &[Value]) {
        n.busy = true;
        self.net("setNetworkRules", json!({"network": n.id(), "rules": rules}), NetReply::View);
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::app::test_app;
    use super::super::network::tests::{key, owner};
    use super::*;

    #[test]
    fn rules_read_with_the_bots_names() {
        let n = owner();
        let deny = json!({"fromComputer": "laptop", "fromAgent": "*", "toAgent": "*", "access": "deny"});
        assert_eq!(n.rule_text(&deny), "Deny: any bot on laptop → any bot here");
        let allow = json!({"fromComputer": "laptop", "fromAgent": "coder", "toAgent": "reviewer", "access": "allow"});
        assert_eq!(n.rule_text(&allow), "Allow: Coder on laptop → Reviewer");
        let level = |access: &str| {
            n.rule_text(&json!({"fromComputer": "*", "fromAgent": "*", "toAgent": "*", "access": access}))
        };
        assert_eq!(level("auto"), "Auto: any bot on any computer → any bot here");
        assert_eq!(level("ask"), "Ask: any bot on any computer → any bot here");
        assert_eq!(level("later"), "Deny: any bot on any computer → any bot here", "never wider than it is");
    }

    #[tokio::test]
    async fn a_new_rule_picks_from_what_the_network_offers() {
        let mut app = test_app();
        let mut n = owner();
        let mut form = RuleForm::default();
        for k in [KeyCode::Right, KeyCode::Tab, KeyCode::Right, KeyCode::Tab, KeyCode::Right] {
            let Step::NewRule(next) = app.rule_form_key(&mut n, form, key(k)) else { panic!("still the form") };
            form = next;
        }
        assert_eq!((form.access, form.from_computer.as_str()), (Access::Auto, "laptop"));
        assert_eq!(form.from_agent, "coder", "only the laptop's agents once it's picked");
        assert!(matches!(app.rule_form_key(&mut n, form, key(KeyCode::Enter)), Step::Rules(_)));
        assert!(n.busy, "saved with setNetworkRules");
    }

    #[tokio::test]
    async fn a_rule_to_a_session_only_allows_or_denies_and_keeps_its_pick() {
        let mut app = test_app();
        let mut n = owner();
        let view = n.view.as_mut().unwrap();
        view["sessions"] = json!([{"address": "api-codex"}, {"address": "web-claude"}]);
        let mut form = RuleForm { access: Access::Ask, field: RuleField::ToAgent, ..RuleForm::default() };
        while form.to_agent != "web-claude" {
            let Step::NewRule(next) = app.rule_form_key(&mut n, form, key(KeyCode::Right)) else { panic!("the form") };
            form = next;
        }
        assert!(n.rule_choices(&form).mine.iter().any(|c| c.0 == "api-codex"), "sessions are offered");
        assert_eq!(form.access, Access::Deny, "a session can't be held to Ask");
        form.field = RuleField::Access;
        let mut seen = Vec::new();
        for _ in 0..3 {
            let Step::NewRule(next) = app.rule_form_key(&mut n, form, key(KeyCode::Right)) else { panic!("the form") };
            form = next;
            seen.push(form.access);
        }
        assert_eq!(seen, [Access::Allow, Access::Deny, Access::Allow]);
        // A session listed before it doesn't move the pick.
        n.view.as_mut().unwrap()["sessions"] = json!([{"address": "new-codex"}, {"address": "web-claude"}]);
        assert_eq!(label(&n.rule_choices(&form).mine, &form.to_agent), "web-claude");
    }
}
