//! Instruction files an agent keeps in the user's home: its global file (`~/.claude/CLAUDE.md`,
//! `~/.codex/AGENTS.md`, …) and the rule folders it reads. A rule in a subfolder is named by its
//! path under the folder, so two `style.mdc` files stay two rows.

use std::path::Path;

use super::{MAX_DEPTH, MAX_ITEMS, NAME_CHARS, TEXT_CHARS, clip, homes, is_dir, read_limited, skipped_name};
use crate::market::front_matter;

use super::Named;

const RULE_BYTES: u64 = 64 * 1024;

pub(super) fn list(backend: &str, home: &Path) -> Vec<Named> {
    let mut out = Vec::new();
    for file in homes(home, files(backend)) {
        let label = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(item) = rule_file(&file, &label) {
            out.push(item);
        }
    }
    for dir in homes(home, dirs(backend)) {
        walk(&dir, &dir, 0, &mut out);
    }
    super::dedupe(&mut out);
    out
}

fn files(backend: &str) -> &'static [&'static str] {
    match backend {
        "claude" => &[".claude/CLAUDE.md"],
        "codex" => &[".codex/AGENTS.md"],
        "copilot" => &[".copilot/copilot-instructions.md"],
        "codebuddy" | "workbuddy" => &[".codebuddy/CODEBUDDY.md", ".codebuddy/AGENTS.md"],
        "gemini" | "antigravity" => &[".gemini/GEMINI.md"],
        "grok" => &[".grok/AGENTS.md"],
        _ => &[],
    }
}

fn dirs(backend: &str) -> &'static [&'static str] {
    match backend {
        "cursor" => &[".cursor/rules"],
        "claude" => &[".claude/rules"],
        "codebuddy" | "workbuddy" => &[".codebuddy/rules"],
        "gemini" | "antigravity" => &[".gemini/rules"],
        "grok" => &[".grok/rules"],
        _ => &[],
    }
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<Named>) {
    if depth > MAX_DEPTH || out.len() >= MAX_ITEMS || !is_dir(dir) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        if out.len() >= MAX_ITEMS {
            return;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if skipped_name(&name) {
            continue;
        }
        let path = entry.path();
        if is_dir(&path) {
            walk(root, &path, depth + 1, out);
        } else if is_rule(&path)
            && let Some(item) = rule_file(&path, &relative(root, &path))
        {
            out.push(item);
        }
    }
}

/// `frontend/style.mdc` for a rule under `root`, with `/` on every platform.
fn relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

fn is_rule(path: &Path) -> bool {
    matches!(path.extension().and_then(|ext| ext.to_str()), Some("md" | "mdc" | "markdown"))
}

fn rule_file(path: &Path, label: &str) -> Option<Named> {
    if !path.is_file() {
        return None;
    }
    let text = read_limited(path, RULE_BYTES)?;
    let (name, description) = front_matter(&text);
    let name = clip(&name.unwrap_or_else(|| label.to_owned()), NAME_CHARS);
    if name.is_empty() {
        return None;
    }
    Some(Named { name, description: clip(&description.unwrap_or_default(), TEXT_CHARS), detail: String::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-gear-rules-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    fn names(rules: &[Named]) -> Vec<&str> {
        let mut names = rules.iter().map(|r| r.name.as_str()).collect::<Vec<_>>();
        names.sort_unstable();
        names
    }

    #[test]
    fn claude_lists_its_global_file_and_same_named_rules() {
        let root = scratch();
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".claude/rules/frontend")).expect("frontend");
        std::fs::create_dir_all(home.join(".claude/rules/backend")).expect("backend");
        std::fs::write(home.join(".claude/CLAUDE.md"), "# Global\n").expect("global");
        std::fs::write(home.join(".claude/rules/frontend/style.md"), "# Front\n").expect("front");
        std::fs::write(home.join(".claude/rules/backend/style.md"), "# Back\n").expect("back");
        let rules = list("claude", &home);
        assert_eq!(names(&rules), ["CLAUDE.md", "backend/style.md", "frontend/style.md"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn codex_lists_its_agents_file() {
        let root = scratch();
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".codex")).expect("codex");
        std::fs::create_dir_all(home.join(".claude")).expect("claude");
        std::fs::write(home.join(".codex/AGENTS.md"), "Be brief.\n").expect("agents");
        std::fs::write(home.join(".claude/CLAUDE.md"), "# Not Codex\n").expect("claude");
        assert_eq!(names(&list("codex", &home)), ["AGENTS.md"]);
        let _ = std::fs::remove_dir_all(root);
    }
}
