//! The palette's "Delete old threads": how many days after it last ended a thread is deleted, never by default
//! (`setThreadKeepDays`, kept by the host for every app; [older threads](../../../../docs/features/work.md#older-threads)).
//! Each choice is a row of its own, so nothing is sent but the one picked.

use std::borrow::Cow;

use serde_json::{Value, json};

use super::super::l10n::tr;
use super::{Action, After, App};

/// The choice as the palette and the flash say it; empty before `hello` (or from a host without it).
fn keep_label(days: Option<u32>) -> String {
    match days {
        None => String::new(),
        Some(0) => tr("Keep forever").into_owned(),
        Some(days) => tr("After {count} days").replace("{count}", &days.to_string()),
    }
}

/// A palette row's words: a "Delete old threads" choice from the phrases the apps translate, any other row its own.
pub fn palette_label(label: &str, action: Action) -> Cow<'_, str> {
    match action {
        Action::ThreadKeep(days) => Cow::Owned(format!("{}: {}", tr("Delete old threads"), keep_label(Some(days)))),
        _ => tr(label),
    }
}

impl App {
    /// Sets the picked choice; nothing before the `hello` says where it stands (or from a host without it).
    pub(super) fn set_thread_keep(&mut self, days: u32) {
        if self.thread_keep_days.is_some() {
            self.call("setThreadKeepDays", json!({"days": days}), After::ThreadKeep);
        }
    }

    pub(super) fn thread_keep_set(&mut self, v: &Value) {
        self.thread_keep_days = days(&v["days"]);
        let shown = format!("{}: {}", tr("Delete old threads"), keep_label(self.thread_keep_days));
        self.flash(&shown);
    }
}

/// `hello.threadKeepDays` or the event's `days`.
pub(super) fn days(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|d| u32::try_from(d).ok())
}

#[cfg(test)]
mod tests {
    use super::super::ACTIONS;
    use super::*;

    #[test]
    fn the_palette_offers_each_choice_as_its_own_row() {
        let offered: Vec<u32> = ACTIONS
            .iter()
            .filter_map(|(_, _, a)| if let Action::ThreadKeep(d) = a { Some(*d) } else { None })
            .collect();
        assert_eq!(offered, [0, 7, 30, 90], "never first, as the apps' menus");
        assert!(palette_label("", Action::ThreadKeep(30)).contains("30"));
        assert_eq!(keep_label(None), "");
        assert_eq!(days(&json!(30)), Some(30));
        assert_eq!(days(&Value::Null), None);
    }
}
