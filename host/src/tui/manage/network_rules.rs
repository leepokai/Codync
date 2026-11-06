//! A network's rules in the network sheet: who on its other computers may message this
//! computer's bots, the first match deciding (`codync_network::rules`).

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::{Value, json};

use super::super::app::{App, cycle};
use super::super::l10n::tr;
use super::network::{NetReply, NetworkSheet, Step};

const ANY: &str = "*";

/// A new rule: each choice cycles through "any" and the names the network offers.
#[derive(Default)]
pub struct RuleForm {
    pub deny: bool,
    /// Index into `NetworkSheet::rule_choices`; 0 is any.
    pub from_computer: usize,
    pub from_agent: usize,
    pub to_agent: usize,
    /// Put it on top instead of after the others.
    pub first: bool,
    pub field: RuleField,
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
        if form.from_computer > 0
            && let Some(c) = others.get(form.from_computer - 1)
        {
            agents.extend(agents_of(c));
        }
        let mut mine = vec![any("Any bot here")];
        if let Some(me) = self.me() {
            mine.extend(agents_of(me));
        }
        Choices { computers, agents, mine }
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
                tr("{bot} on {computer}").replace("{bot}", name).replace("{computer}", computer)
            }
        };
        let to = match s(rule, "toAgent").as_str() {
            ANY => tr("any bot here").into_owned(),
            id => self.me().and_then(|me| agent_name(me, id)).unwrap_or(id).to_owned(),
        };
        let template = if rule["access"] == "deny" { "Deny: {from} → {to}" } else { "Allow: {from} → {to}" };
        tr(template).replace("{from}", &from).replace("{to}", &to)
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
                match form.field {
                    RuleField::Access => form.deny = !form.deny,
                    RuleField::FromComputer => {
                        form.from_computer = cycle(form.from_computer, by, choices.computers.len());
                        form.from_agent = 0;
                    }
                    RuleField::FromAgent => form.from_agent = cycle(form.from_agent, by, choices.agents.len()),
                    RuleField::ToAgent => form.to_agent = cycle(form.to_agent, by, choices.mine.len()),
                    RuleField::First => form.first = !form.first,
                }
            }
            KeyCode::Enter => {
                let pick =
                    |list: &[(String, String)], i: usize| list.get(i).map_or_else(|| ANY.to_owned(), |c| c.0.clone());
                let rule = json!({
                    "fromComputer": pick(&choices.computers, form.from_computer),
                    "fromAgent": pick(&choices.agents, form.from_agent),
                    "toAgent": pick(&choices.mine, form.to_agent),
                    "access": if form.deny { "deny" } else { "allow" },
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
        let choices = n.rule_choices(&form);
        assert_eq!((form.deny, choices.computers[form.from_computer].0.as_str()), (true, "laptop"));
        assert_eq!(choices.agents[form.from_agent].0, "coder", "only the laptop's agents once it's picked");
        assert!(matches!(app.rule_form_key(&mut n, form, key(KeyCode::Enter)), Step::Rules(_)));
        assert!(n.busy, "saved with setNetworkRules");
    }
}
