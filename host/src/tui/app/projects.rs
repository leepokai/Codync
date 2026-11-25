//! The bot form's Projects and Worktree mode fields, on a host with projects (`hello.projects`): the folders the bot's
//! threads work in, the first its default, and when a thread gets a worktree of its own. ←/→ picks a project, ↵ adds
//! one with the folder picker, `d` makes the picked one the default (first), `x` removes it. Shared folders (on a host
//! that fills worktrees) edits the picked git project's folders linked into each new worktree: a name typed and ↵ adds
//! it, ←/→ picks one, Backspace on an empty field removes it.

use crossterm::event::{KeyCode, KeyEvent};

use super::{App, Form, cycle};
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
                let gone = projects.remove(self.sub);
                self.sub = self.sub.min(projects.len().saturating_sub(1));
                if let Some(shared) = self.shared_folders.as_mut() {
                    shared.remove(&gone);
                }
            }
            KeyCode::Char('d') if self.sub < projects.len() => {
                let project = projects.remove(self.sub);
                projects.insert(0, project);
                self.sub = 0;
            }
            _ => return false,
        }
        self.project = self.sub;
        true
    }

    /// A key on the Shared folders field of `project` it handles itself.
    pub(super) fn shared_key(&mut self, project: &str, k: KeyEvent) -> bool {
        let Some(shared) = self.shared_folders.as_mut() else { return false };
        let typing = !self.shared_name.text.is_empty();
        let names = shared.entry(project.to_owned()).or_default();
        let used = match k.code {
            KeyCode::Left | KeyCode::Right if !typing && !names.is_empty() => {
                self.sub = cycle(self.sub, if k.code == KeyCode::Left { -1 } else { 1 }, names.len());
                true
            }
            KeyCode::Backspace | KeyCode::Delete if !typing && self.sub < names.len() => {
                names.remove(self.sub);
                self.sub = self.sub.min(names.len().saturating_sub(1));
                true
            }
            KeyCode::Enter if typing => {
                let name = self.shared_name.text.trim().to_owned();
                self.shared_name.clear();
                // The host refuses an absolute name or one with `..`; its reason shows on save.
                if !name.is_empty() && !names.contains(&name) {
                    names.push(name);
                }
                self.sub = names.len().saturating_sub(1);
                true
            }
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down | KeyCode::Esc | KeyCode::Enter => false,
            _ => self.shared_name.key(k),
        };
        // An emptied list goes, so adding and removing a name leaves the form unchanged.
        shared.retain(|_, names| !names.is_empty());
        used
    }

    /// Adds the folder picked for a new project, once; the pick moves to it.
    pub(super) fn add_project(&mut self, path: String) {
        let Some(projects) = self.projects.as_mut() else { return };
        self.sub = projects.iter().position(|p| *p == path).unwrap_or_else(|| {
            projects.push(path);
            projects.len() - 1
        });
        self.project = self.sub;
    }

    pub(super) fn cycle_worktree_mode(&mut self, d: isize) {
        let i = WORKTREE_MODES.iter().position(|m| *m == self.worktree_mode).unwrap_or(1);
        self.worktree_mode = WORKTREE_MODES[cycle(i, d, WORKTREE_MODES.len())];
    }
}

impl App {
    /// The git project the Shared folders field edits: the one picked on Projects, else the first in a git repository;
    /// `None` on a host that doesn't fill worktrees.
    pub(in crate::tui) fn shared_project<'a>(&self, f: &'a Form) -> Option<&'a str> {
        f.shared_folders.as_ref()?;
        let bot = self.bots.get(&f.bot_id)?;
        let projects = f.projects.as_deref()?;
        let git = |p: &&String| bot.git_project(p);
        projects.get(f.project).filter(git).or_else(|| projects.iter().find(git)).map(String::as_str)
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
            "projectGitRepos": [true, false], "worktreeMode": "sideBySide", "sharedFolders": {"/p/a": ["node_modules"]}});
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

    /// The editor on a host that fills worktrees, `/p/a` sharing `node_modules`.
    fn filling_editor(app: &mut App, field: Field) -> Box<super::Form> {
        app.projects = true;
        app.worktree_fill = true;
        editor(app, field)
    }

    #[tokio::test]
    async fn shared_folders_are_typed_added_and_removed_and_saved_whole() {
        let mut app = test_app();
        let f = filling_editor(&mut app, Field::SharedFolders);
        assert!(app.field_shown(&f, Field::SharedFolders));
        assert_eq!(app.shared_project(&f), Some("/p/a"));

        let f = press(&mut app, f, ".cache/pip".chars().map(KeyCode::Char).chain([KeyCode::Enter]));
        assert_eq!(f.changes(), json!({"sharedFolders": {"/p/a": ["node_modules", ".cache/pip"]}}));

        let f = press(&mut app, f, [KeyCode::Left, KeyCode::Backspace]);
        assert_eq!(f.changes(), json!({"sharedFolders": {"/p/a": [".cache/pip"]}}), "the picked one goes");

        let f = press(&mut app, f, [KeyCode::Backspace]);
        assert_eq!(f.changes(), json!({"sharedFolders": {}}), "an emptied list goes");

        // `d` and `x` are letters of a name here, and ↵ with nothing typed saves.
        let f = press(&mut app, f, "node_modules".chars().map(KeyCode::Char).chain([KeyCode::Enter]));
        assert_eq!(f.changes(), json!({}), "back as opened");
        assert!(
            app.form_key(f, KeyEvent::from(KeyCode::Enter)).is_some_and(|o| matches!(o, Overlay::Form(f) if f.saving))
        );
    }

    #[tokio::test]
    async fn shared_folders_follow_the_picked_git_project_and_go_with_it() {
        let mut app = test_app();
        let f = filling_editor(&mut app, Field::Projects);
        let f = press(&mut app, f, [KeyCode::Right]);
        assert_eq!(app.shared_project(&f), Some("/p/a"), "/p/b isn't in a git repository: the first that is");

        let f = press(&mut app, f, [KeyCode::Left, KeyCode::Char('x')]);
        assert_eq!(f.changes(), json!({"projects": ["/p/b"], "sharedFolders": {}}), "its list goes with it");
        assert!(!app.field_shown(&f, Field::SharedFolders), "no git project left");
    }

    #[tokio::test]
    async fn a_host_that_doesnt_fill_worktrees_has_no_shared_folders() {
        let mut app = test_app();
        app.projects = true;
        let f = editor(&mut app, Field::SharedFolders);
        assert!(!app.field_shown(&f, Field::SharedFolders));
        assert!(f.shared_folders.is_none() && app.shared_project(&f).is_none());
        let mut f = f;
        f.projects = Some(vec!["/p/b".into()]);
        assert!(f.changes().get("sharedFolders").is_none(), "{}", f.changes());
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
