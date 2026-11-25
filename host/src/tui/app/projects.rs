//! The bot form's Projects and Worktree mode fields, on a host with projects (`hello.projects`): the folders the bot's
//! threads work in, the first its default, and when a thread gets a worktree of its own. ←/→ picks a project, ↵ adds
//! one with the folder picker, `d` makes the picked one the default (first), `x` removes it.

use crossterm::event::{KeyCode, KeyEvent};

use super::{Form, cycle};
use crate::store::WorktreeMode;

/// The worktree modes in the order ←/→ steps through them.
pub const WORKTREE_MODES: [WorktreeMode; 3] = [WorktreeMode::Off, WorktreeMode::SideBySide, WorktreeMode::Always];

impl Form {
    /// A key on the Projects field it handles itself.
    pub(super) fn project_key(&mut self, k: KeyEvent) -> bool {
        let Some(projects) = self.projects.as_mut() else { return false };
        match k.code {
            KeyCode::Left | KeyCode::Right if !projects.is_empty() => {
                self.sub = cycle(self.sub, if k.code == KeyCode::Left { -1 } else { 1 }, projects.len());
            }
            KeyCode::Char('x') if self.sub < projects.len() => {
                projects.remove(self.sub);
                self.sub = self.sub.min(projects.len().saturating_sub(1));
            }
            KeyCode::Char('d') if self.sub < projects.len() => {
                let project = projects.remove(self.sub);
                projects.insert(0, project);
                self.sub = 0;
            }
            _ => return false,
        }
        true
    }

    /// Adds the folder picked for a new project, once; the pick moves to it.
    pub(super) fn add_project(&mut self, path: String) {
        let Some(projects) = self.projects.as_mut() else { return };
        self.sub = projects.iter().position(|p| *p == path).unwrap_or_else(|| {
            projects.push(path);
            projects.len() - 1
        });
    }

    pub(super) fn cycle_worktree_mode(&mut self, d: isize) {
        let i = WORKTREE_MODES.iter().position(|m| *m == self.worktree_mode).unwrap_or(1);
        self.worktree_mode = WORKTREE_MODES[cycle(i, d, WORKTREE_MODES.len())];
    }
}

#[cfg(test)]
mod tests {
    use super::super::{App, Bot, FIELDS, Field, Overlay, test_app};
    use crossterm::event::{KeyCode, KeyEvent};
    use serde_json::{Value, json};

    /// The editor of a bot with projects `/p/a` (a git repository) and `/p/b`, on `field`.
    fn editor(app: &mut App, field: Field) -> Box<super::Form> {
        let bot = json!({"id": "b", "name": "B", "backend": "claude", "cwd": "/home/.codync/bots/b/workspace",
            "managedWorkspace": true, "gitRepo": true, "threadWorktrees": true, "projects": ["/p/a", "/p/b"],
            "projectGitRepos": [true, false], "worktreeMode": "sideBySide"});
        app.bots.insert("b".into(), Bot::parse(&bot).expect("a bot"));
        app.selected = Some("b".into());
        app.edit_bot();
        let Some(Overlay::Form(mut f)) = app.overlays.pop() else { panic!("the editor opens") };
        f.field = FIELDS.iter().position(|x| *x == field).expect("the field");
        f
    }

    fn press(app: &mut App, mut f: Box<super::Form>, keys: impl IntoIterator<Item = KeyCode>) -> Box<super::Form> {
        for key in keys {
            let Some(Overlay::Form(next)) = app.form_key(f, KeyEvent::from(key)) else { panic!("the editor stays") };
            f = next;
        }
        f
    }

    #[tokio::test]
    async fn projects_are_added_removed_and_made_default_and_saved_whole() {
        let mut app = test_app();
        app.projects = true;
        let f = editor(&mut app, Field::Projects);
        assert_eq!(f.changes(), json!({}));
        assert!(!app.field_shown(&f, Field::ThreadWorktrees), "the mode replaces the switch");

        let f = press(&mut app, f, [KeyCode::Right, KeyCode::Char('d')]);
        assert_eq!(f.changes(), json!({"projects": ["/p/b", "/p/a"]}), "the picked one moves first");

        let f = press(&mut app, f, [KeyCode::Char('x')]);
        assert_eq!(f.changes(), json!({"projects": ["/p/a"]}));

        // ↵ opens the folder picker; its pick comes back to the form.
        assert!(app.form_key(f, KeyEvent::from(KeyCode::Enter)).is_none());
        assert!(matches!(app.overlays.pop(), Some(Overlay::Folder(_))), "the picker opens over the form");
        app.folder_chosen("/p/c".into());
        app.folder_chosen("/p/c".into());
        let Some(Overlay::Form(f)) = app.overlays.pop() else { panic!("the form") };
        assert_eq!((f.changes(), f.sub), (json!({"projects": ["/p/a", "/p/c"]}), 1), "added once, picked");
    }

    #[tokio::test]
    async fn the_worktree_mode_steps_through_three_and_shows_only_with_a_git_project() {
        let mut app = test_app();
        app.projects = true;
        let f = editor(&mut app, Field::WorktreeMode);
        assert!(app.field_shown(&f, Field::WorktreeMode));
        let f = press(&mut app, f, [KeyCode::Right]);
        assert_eq!(f.changes(), json!({"worktreeMode": "always"}));
        let f = press(&mut app, f, [KeyCode::Right]);
        assert_eq!(f.changes(), json!({"worktreeMode": "off"}));
        let mut f = press(&mut app, f, [KeyCode::Left, KeyCode::Left]);
        assert_eq!(f.changes(), json!({}), "back to side by side");
        f.projects = Some(vec!["/p/b".into()]);
        assert!(!app.field_shown(&f, Field::WorktreeMode), "no project in a git repository");
        let body = f.changes();
        assert!(body.get("cwd").is_none() && body.get("threadWorktrees").is_none(), "{body}");
    }

    #[tokio::test]
    async fn an_older_host_keeps_its_folder_and_switch_and_never_gets_projects() {
        let mut app = test_app();
        let mut f = editor(&mut app, Field::Folder);
        assert!(!app.field_shown(&f, Field::Projects) && !app.field_shown(&f, Field::WorktreeMode));
        f.cwd = "/p/new".into();
        let body: Value = f.changes();
        assert_eq!(body, json!({"cwd": "/p/new"}));
    }
}
