//! Host-managed account choices; replies are scoped to one form generation.
use super::super::app::{Form, Overlay};
use super::{App, Reply};
use serde_json::{Value, json};

impl App {
    pub(in crate::tui) fn load_accounts(&mut self, form: &mut Form, authenticate: bool) {
        if form.backend != "antigravity" || form.account_busy {
            return;
        }
        form.account_busy = true;
        form.error = None;
        self.sheet(
            if authenticate { "accountAuthenticate" } else { "listAccounts" },
            json!({"backend": form.backend}),
            Reply::Accounts { scope: form.account_scope.clone(), authenticate },
        );
    }

    pub(in crate::tui) fn cycle_account(&mut self, form: &mut Form, direction: isize) {
        if form.backend != "antigravity" || form.account_busy {
            return;
        }
        let Some(accounts) = form.accounts.as_ref() else {
            self.load_accounts(form, false);
            return;
        };
        let choices: Vec<Option<String>> =
            std::iter::once(None).chain(accounts.iter().map(|(id, _)| Some(id.clone()))).collect();
        let current = choices.iter().position(|id| *id == form.account).unwrap_or(0);
        form.account.clone_from(&choices[super::super::app::cycle(current, direction, choices.len())]);
        form.model.clear();
        form.model_options = Default::default();
        self.want_models_for(&form.backend, form.account.as_deref());
    }

    pub(super) fn accounts_reply(&mut self, scope: &str, authenticate: bool, value: &Value) {
        let Some(Overlay::Form(form)) = self.overlays.last_mut() else {
            return;
        };
        if form.account_scope != scope || form.backend != "antigravity" {
            return;
        }
        form.account_busy = false;
        if authenticate {
            if let Some(id) = value["account"]["id"].as_str() {
                form.account = Some(id.to_owned());
                form.model.clear();
                form.model_options = Default::default();
            }
        } else {
            form.accounts = Some(
                value["accounts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|account| {
                        Some((account["id"].as_str()?.to_owned(), account["name"].as_str()?.to_owned()))
                    })
                    .collect(),
            );
        }
        let backend = form.backend.clone();
        let account = form.account.clone();
        if authenticate {
            form.account_busy = true;
            self.sheet(
                "listAccounts",
                json!({"backend": backend}),
                Reply::Accounts { scope: scope.to_owned(), authenticate: false },
            );
        }
        self.want_models_for(&backend, account.as_deref());
    }
}
