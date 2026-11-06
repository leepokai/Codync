//! The names this computer's bots go by in addresses: `Code Reviewer` is `code-reviewer`, and
//! two bots with one name are `reviewer` and `reviewer-2`. A name once given never goes to
//! another bot, so a letter on its way to a renamed or deleted bot can't reach the wrong one.

use std::collections::HashMap;

const MAX_LEN: usize = 40;

/// A bot or computer name as an address holds it: lowercase letters and digits (any script),
/// `_`, and dashes for everything else.
pub fn handle(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() || c == '_' {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(MAX_LEN).collect();
    if out.is_empty() { "bot".into() } else { out }
}

/// `base`, or the first of `base-2`, `base-3`… that is `free`, when at most `taken` names aren't.
pub fn numbered(base: &str, taken: usize, free: impl Fn(&str) -> bool) -> String {
    std::iter::once(base.to_owned())
        // One more number than there are names taken: one of them is free.
        .chain((2..=taken + 2).map(|n| format!("{base}-{n}")))
        .find(|name| free(name))
        .expect("a number is free")
}

/// Address names both ways, for the bots this computer offers.
#[derive(Default)]
pub struct Handles {
    to_bot: HashMap<String, String>,
    to_handle: HashMap<String, String>,
}

impl Handles {
    /// Names for `(bot id, bot name)`: its name's, or the first numbered one no other bot was ever
    /// given. `given` (name → bot) holds every name handed out so far and takes the new ones.
    pub fn of(bots: impl IntoIterator<Item = (String, String)>, given: &mut HashMap<String, String>) -> Self {
        let mut handles = Self::default();
        for (id, name) in bots {
            let free = numbered(&handle(&name), given.len(), |h| given.get(h).is_none_or(|bot| *bot == id));
            given.insert(free.clone(), id.clone());
            handles.to_bot.insert(free.clone(), id.clone());
            handles.to_handle.insert(id, free);
        }
        handles
    }

    pub fn bot(&self, handle: &str) -> Option<String> {
        self.to_bot.get(&handle.to_lowercase()).cloned()
    }

    pub fn handle(&self, bot: &str) -> Option<String> {
        self.to_handle.get(bot).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_address_names() {
        assert_eq!(handle("Code Reviewer"), "code-reviewer");
        assert_eq!(handle("  Owen's bot!! "), "owen-s-bot");
        assert_eq!(handle("審查員"), "審查員");
        assert_eq!(handle("@/."), "bot");
        assert_eq!(handle(&"x".repeat(60)).len(), MAX_LEN);
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let mut given = HashMap::new();
        let h = Handles::of(
            [("1".into(), "Reviewer".into()), ("2".into(), "reviewer".into()), ("3".into(), "Coder".into())],
            &mut given,
        );
        assert_eq!((h.handle("1"), h.handle("2")), (Some("reviewer".into()), Some("reviewer-2".into())));
        assert_eq!(h.bot("Reviewer-2").as_deref(), Some("2"));
        assert_eq!(h.bot("nobody"), None);
    }

    #[test]
    fn a_name_never_moves_to_another_bot() {
        let mut given = HashMap::new();
        Handles::of([("1".into(), "Reviewer".into()), ("2".into(), "Reviewer".into())], &mut given);
        // 1 is deleted and 2 listed first: each keeps its own name.
        let h = Handles::of([("2".into(), "Reviewer".into())], &mut given);
        assert_eq!(h.handle("2").as_deref(), Some("reviewer-2"));
        // 2 is renamed, then a new bot takes its old name: the new one gets a fresh number.
        let h = Handles::of([("2".into(), "Coder".into()), ("4".into(), "Reviewer".into())], &mut given);
        assert_eq!((h.handle("2"), h.handle("4")), (Some("coder".into()), Some("reviewer-3".into())));
    }
}
