//! Skill folders an agent keeps in the user's home. A symlink to a folder is followed;
//! `MAX_DEPTH` stops a loop. Claude reads `~/.claude/skills` only. Codex and Cursor also
//! read `~/.agents/skills`. Copilot keeps its own `~/.copilot/skills`.

use std::path::Path;

use super::{
    MAX_DEPTH, MAX_ITEMS, NAME_CHARS, SKILL_BYTES, TEXT_CHARS, clip, homes, is_dir, read_limited, skipped_name,
};
use crate::market::front_matter;

use super::Named;

pub(super) fn list(backend: &str, home: &Path) -> Vec<Named> {
    let mut out = Vec::new();
    for root in homes(home, roots(backend)) {
        walk(&root, 0, &mut out);
    }
    super::dedupe(&mut out);
    out
}

fn roots(backend: &str) -> &'static [&'static str] {
    match backend {
        "cursor" => &[
            ".cursor/skills-cursor",
            ".cursor/skills",
            ".cursor/cloud-skills",
            ".agents/skills",
            ".claude/skills",
            ".codex/skills",
            ".grok/skills",
        ],
        "claude" => &[".claude/skills"],
        "copilot" => &[".copilot/skills"],
        "codex" => &[".codex/skills", ".agents/skills"],
        "grok" => &[".grok/skills", ".agents/skills"],
        "codebuddy" | "workbuddy" => &[".codebuddy/skills", ".claude/skills", ".agents/skills"],
        "gemini" | "antigravity" => &[".gemini/skills", ".agents/skills"],
        _ => &[".agents/skills"],
    }
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<Named>) {
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
        if !is_dir(&path) {
            continue;
        }
        let skill = path.join("SKILL.md");
        if skill.is_file() {
            if let Some(item) = skill_item(&skill, &name) {
                out.push(item);
            }
        } else {
            walk(&path, depth + 1, out);
        }
    }
}

fn skill_item(path: &Path, folder: &str) -> Option<Named> {
    let text = read_limited(path, SKILL_BYTES)?;
    let (name, description) = front_matter(&text);
    let name = clip(&name.unwrap_or_else(|| folder.to_owned()), NAME_CHARS);
    if name.is_empty() {
        return None;
    }
    Some(Named { name, description: clip(&description.unwrap_or_default(), TEXT_CHARS), detail: String::new() })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A directory link. Windows has no `std::os::unix`.
    fn link_dir(target: &Path, link: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).expect("symlink");
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(target, link).expect("symlink");
    }

    fn scratch() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("codync-gear-skills-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn claude_reads_its_own_skills_and_follows_a_symlink() {
        let root = scratch();
        let home = root.join("home");
        let real = root.join("real");
        std::fs::create_dir_all(home.join(".claude/skills")).expect("claude");
        std::fs::create_dir_all(home.join(".agents/skills/imported")).expect("agents");
        std::fs::create_dir_all(&real).expect("real");
        std::fs::write(home.join(".agents/skills/imported/SKILL.md"), "---\nname: imported\ndescription: No\n---\n")
            .expect("imported");
        std::fs::write(real.join("SKILL.md"), "---\nname: linked\ndescription: >\n  Review\n  a diff\n---\n# 说明\n")
            .expect("linked");
        link_dir(&real, &home.join(".claude/skills/linked"));
        let skills = list("claude", &home);
        assert_eq!(skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["linked"]);
        assert_eq!(skills[0].description, "Review a diff");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn copilot_does_not_read_claude_skills() {
        let root = scratch();
        let home = root.join("home");
        std::fs::create_dir_all(home.join(".copilot/skills/ship")).expect("copilot");
        std::fs::create_dir_all(home.join(".claude/skills/other")).expect("claude");
        std::fs::write(home.join(".copilot/skills/ship/SKILL.md"), "---\nname: ship\ndescription: Ship it\n---\n")
            .expect("ship");
        std::fs::write(home.join(".claude/skills/other/SKILL.md"), "---\nname: other\ndescription: No\n---\n")
            .expect("other");
        let skills = list("copilot", &home);
        assert_eq!(skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["ship"]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_skill_file_cut_inside_a_character_still_counts() {
        let root = scratch();
        let home = root.join("home");
        let dir = home.join(".claude/skills/notes");
        std::fs::create_dir_all(&dir).expect("dir");
        // 8192 bytes lands inside the last 你, which used to drop the whole file.
        std::fs::write(dir.join("SKILL.md"), "你".repeat(2731)).expect("skill");
        let skills = list("claude", &home);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "notes");
        let _ = std::fs::remove_dir_all(root);
    }
}
