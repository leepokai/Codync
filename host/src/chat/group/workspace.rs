//! A group's shared workspace folder (docs/features/group-workspace.md): the folders it lends to its members'
//! sessions, the room's rules about it, and the once-per-breakage notice when it's gone.

use crate::LockExt;
use crate::hub::Hub;
use crate::store::{BotConfig, Store};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;

/// The shared workspaces of the groups `bot` belongs to (tasks pass their owner), for its sessions'
/// `additionalDirectories`. A folder that is gone is left out: an agent refuses to open a session on a folder it
/// can't see, and the room tells the member the folder is missing instead.
pub fn folders(store: &Store, bot: &str) -> Vec<String> {
    match store.bots() {
        Ok(rows) => {
            let mut folders: Vec<String> = rows
                .into_iter()
                .filter(|row| !row.deleted && row.config.is_group())
                .filter(|row| row.config.members.iter().any(|member| member == bot))
                .filter_map(|row| row.config.workspace)
                .filter(|workspace| Path::new(workspace).is_dir())
                .collect();
            folders.sort();
            folders.dedup();
            folders
        }
        Err(error) => {
            tracing::warn!(bot, error = format!("{error:#}"), "couldn't list bots for group workspaces");
            Vec::new()
        }
    }
}

/// What a member's turn says about the room's shared folder; `None` for a group without one.
pub fn section(hub: &Hub, group: &BotConfig) -> Option<String> {
    let workspace = group.workspace.as_deref()?;
    // Keyed by folder too: switching the folder starts a fresh breakage count.
    let latch = format!("{}\u{0}{workspace}", group.id);
    let text = if Path::new(workspace).is_dir() {
        hub.group_workspaces.present(&latch);
        format!(
            "The team shares the folder {workspace}. Keep joint output there (your own folder is for work \
             nobody else needs). It changes between your turns: look before you write, never overwrite work \
             you have not read, and prefer small, scoped edits. Before substantially rewriting a file a \
             teammate is actively working on, say so in the room. If it is a repository, commit early; \
             never merge or rebase for someone else."
        )
    } else {
        // Once per breakage, not once per member or round.
        if hub.group_workspaces.first_missing(&latch) {
            crate::analytics::capture(hub, crate::analytics::Event::GroupWorkspaceUnavailable, serde_json::Map::new());
        }
        format!(
            "The team's shared folder {workspace} is missing right now; say so in the room and keep working \
             in your own folder until the user fixes it."
        )
    };
    Some(text)
}

/// Groups whose shared folder was seen missing: one notice per breakage, until it is seen there again.
#[derive(Default)]
pub struct Workspaces {
    missing: Mutex<HashSet<String>>,
}

impl Workspaces {
    /// The folder behind `key` was just seen missing: `true` the first time, while it stays missing.
    fn first_missing(&self, key: &str) -> bool {
        self.missing.locked().insert(key.to_owned())
    }

    /// The folder behind `key` is there again: another breakage reports once more.
    fn present(&self, key: &str) {
        self.missing.locked().remove(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    fn group_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-gws-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn folders_follow_membership_and_skip_what_is_gone() {
        let store = Store::open(Path::new(":memory:")).unwrap();
        let save =
            |body: serde_json::Value| store.save_bot(&serde_json::from_value::<BotConfig>(body).unwrap()).unwrap();
        let (one, two, gone) = (group_dir("one"), group_dir("two"), group_dir("gone"));
        save(json!({"id": "a", "name": "a", "backend": "claude", "cwd": "/tmp"}));
        save(json!({"id": "b", "name": "b", "backend": "claude", "cwd": "/tmp"}));
        save(json!({"id": "g1", "kind": "group", "name": "one", "members": ["a", "b"], "workspace": one}));
        save(json!({"id": "g2", "kind": "group", "name": "two", "members": ["b"], "workspace": two}));
        save(json!({"id": "g3", "kind": "group", "name": "three", "members": ["a"], "workspace": gone}));
        store.delete_bot("g3").unwrap();
        let mut sorted = [one.clone(), two.clone()];
        sorted.sort();
        assert_eq!(folders(&store, "a"), [one.to_string_lossy().into_owned()], "its own groups only, deleted ones out");
        assert_eq!(
            folders(&store, "b"),
            sorted.iter().map(|d| d.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            "union of a member's groups, sorted"
        );
        assert_eq!(folders(&store, "nobody"), Vec::<String>::new());

        // A saved folder that is gone isn't handed to a session at all.
        save(json!({"id": "g4", "kind": "group", "name": "four", "members": ["a"], "workspace": gone}));
        std::fs::remove_dir_all(&gone).unwrap();
        assert_eq!(folders(&store, "a"), [one.to_string_lossy().into_owned()]);

        for dir in [one, two] {
            std::fs::remove_dir_all(dir).ok();
        }
    }

    #[test]
    fn a_missing_folder_reports_once_per_breakage() {
        let workspaces = Workspaces::default();
        assert!(workspaces.first_missing("g"));
        assert!(!workspaces.first_missing("g"), "one notice per breakage, not one per member and round");
        workspaces.present("g");
        assert!(workspaces.first_missing("g"), "a new breakage reports again");
    }
}
