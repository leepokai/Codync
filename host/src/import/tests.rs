use super::archive::Archive;
use super::manifest;
use serde_json::{Value, json};
use std::io::Write as _;
use std::path::PathBuf;

/// A zip at a fresh temporary path holding `files` (path, contents).
fn zip(files: &[(&str, &[u8])]) -> PathBuf {
    let path = std::env::temp_dir().join(format!("codync-import-{}.zip", uuid::Uuid::new_v4()));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for (name, data) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
    path
}

fn read(files: &[(&str, &[u8])]) -> anyhow::Result<(manifest::Plan, Vec<String>)> {
    let path = zip(files);
    let mut archive = Archive::open(&path)?;
    let plan = manifest::read(&mut archive, 1_700_000_000_000);
    std::fs::remove_file(path).unwrap();
    Ok((plan?, archive.skipped))
}

fn export() -> Value {
    json!({
        "format": "codync-bot-import", "version": 1, "source": {"app": "grok-bot"},
        "agents": [
            {"key": "research", "name": "Research", "title": "Analyst", "description": "Find sources.",
             "avatar": "agents/research/avatar.png", "skills": ["agents/research/skills/brief", "agents/sales/skills/x"],
             "routines": [
                {"name": "Digest", "instruction": "Summarize.", "active": false, "timezone": "Asia/Taipei",
                 "schedule": {"kind": "weekdays", "time": "08:30"}},
                {"name": "Broken", "instruction": "x", "schedule": {"kind": "fortnightly"}}
             ],
             "memories": ["The user reports in EUR.", ""], "files": "agents/research/files"},
            {"key": "Bad Key", "name": "Bad"},
            {"key": "sales", "name": "Sales", "description": "Sell."}
        ],
        "channels": [
            {"key": "desk", "name": "Desk", "members": ["sales", "research", "ghost"], "lead": "research",
             "memories": ["Actions, not summaries."]},
            {"key": "empty", "name": "Empty", "members": ["ghost"]}
        ]
    })
}

#[test]
fn an_export_reads_into_bots_and_groups() {
    let manifest = export().to_string();
    let (plan, skipped) = read(&[
        ("codync-import.json", manifest.as_bytes()),
        ("agents/research/avatar.png", b"png"),
        ("agents/research/skills/brief/SKILL.md", b"---\nname: Brief\ndescription: Write briefs\n---\nDo it."),
        ("agents/research/files/notes.md", b"notes"),
        ("agents/research/files/.env", b"SECRET=1"),
        ("__MACOSX/agents/._x", b""),
    ])
    .unwrap();
    assert_eq!(plan.source, "grok-bot");
    assert_eq!(skipped, ["agents/research/files/.env"]);
    let keys: Vec<&str> = plan.bots.iter().map(|b| b.key.as_str()).collect();
    assert_eq!(keys, ["research", "sales"], "the agent with a bad key is left out");
    let research = &plan.bots[0];
    assert_eq!(research.description, "Analyst\n\nFind sources.");
    assert_eq!(research.avatar.as_deref(), Some("agents/research/avatar.png"));
    assert_eq!(research.skills, ["agents/research/skills/brief"], "another agent's folder is refused");
    assert_eq!(research.files.as_deref(), Some("agents/research/files"));
    assert_eq!(research.memories, ["The user reports in EUR."]);
    assert_eq!(research.routines.len(), 1);
    assert!(!research.routines[0].enabled);
    assert_eq!(plan.groups.len(), 1, "a group with no exported agent is left out");
    assert_eq!(plan.groups[0].members, ["research", "sales"], "the lead first, unknown agents dropped");
    let warnings = plan.warnings.join("\n");
    for expected in ["Bad", "Broken", "agents/sales/skills/x", "Empty"] {
        assert!(warnings.contains(expected), "{expected} in {warnings}");
    }
}

#[test]
fn other_apps_exports_and_a_wrapping_folder_are_read() {
    let mut agent = export();
    agent["format"] = "openbot-agent-import".into();
    let (plan, _) = read(&[("export/openbot-import.json", agent.to_string().as_bytes())]).unwrap();
    assert_eq!(plan.bots.len(), 2);

    let transfer = json!({
        "format": "openbot-instance-transfer", "version": 1, "source": {"appVersion": "1.0", "exportedAt": null},
        "agents": [{"key": "ops", "name": "Ops", "title": "", "description": "Keep it running.",
                    "provider": "codex", "skills": [], "files": null, "routines": [],
                    "oneTimeRoutines": [{"key": "r", "name": "Launch", "instruction": "Ship.", "active": true,
                                         "timezone": "UTC", "schedule": {"kind": "once", "date": "2031-01-01", "time": "09:00"}}],
                    "memories": [{"text": "Deploys on Fridays.", "inclusion": "essential"}]}],
        "settings": {"sidebarSections": [{"name": "Work", "agentKeys": ["ops"]}]}
    });
    let (plan, _) = read(&[("openbot-transfer.json", transfer.to_string().as_bytes())]).unwrap();
    let ops = &plan.bots[0];
    assert_eq!(plan.source, "openbot");
    assert_eq!(ops.memories, ["Deploys on Fridays."]);
    assert_eq!(ops.routines.len(), 1, "one-time routines come along");
    assert_eq!(ops.section.as_deref(), Some("Work"));
    assert_eq!(ops.backend.as_deref(), Some("codex"));
}

#[test]
fn files_that_are_not_an_export_are_refused() {
    assert!(read(&[("readme.md", b"hi")]).is_err(), "no manifest");
    let mut newer = export();
    newer["version"] = 2.into();
    let err = read(&[("codync-import.json", newer.to_string().as_bytes())]).unwrap_err();
    assert!(format!("{err:#}").contains("newer version"));
    let mut wrong = export();
    wrong["format"] = "something-else".into();
    assert!(read(&[("codync-import.json", wrong.to_string().as_bytes())]).is_err());
    let none = json!({"format": "codync-bot-import", "version": 1, "agents": [{"key": "-x", "name": "X"}]});
    assert!(read(&[("codync-import.json", none.to_string().as_bytes())]).is_err(), "no importable agent");
}

#[test]
fn extract_keeps_paths_below_the_folder() {
    let path = zip(&[("a/files/x.md", b"x"), ("a/files/d/y.md", b"y"), ("a/other.md", b"o")]);
    let mut archive = Archive::open(&path).unwrap();
    let dest = std::env::temp_dir().join(format!("codync-import-out-{}", uuid::Uuid::new_v4()));
    assert_eq!(archive.extract("a/files", &dest).unwrap(), 2);
    assert_eq!(std::fs::read_to_string(dest.join("d/y.md")).unwrap(), "y");
    assert!(!dest.join("other.md").exists());
    std::fs::remove_dir_all(dest).unwrap();
    std::fs::remove_file(path).unwrap();
}

#[test]
fn entries_that_leave_the_archive_or_link_are_skipped() {
    let path = std::env::temp_dir().join(format!("codync-import-{}.zip", uuid::Uuid::new_v4()));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    for name in ["../evil.md", "/abs.md", "ok.md"] {
        zip.start_file(name, options).unwrap();
        zip.write_all(b"x").unwrap();
    }
    zip.add_symlink("link", "/etc/passwd", options).unwrap();
    zip.finish().unwrap();
    let archive = Archive::open(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(archive.has("ok.md") && !archive.has("/abs.md"), "an absolute path stays below the archive's root");
    assert_eq!(archive.skipped, ["../evil.md", "link"]);
}

#[tokio::test]
async fn an_import_makes_bots_and_one_group_and_rolls_a_failed_bot_back() {
    let dir = std::env::temp_dir().join(format!("codync-import-hub-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let store = crate::store::Store::open(&dir.join("t.db")).unwrap();
    let identity = crate::remote::identity::Identity::load_or_create(&dir).unwrap();
    let hub = crate::hub::Hub::new(store, "test".into(), identity, "token".into(), 0);
    let mut manifest = export();
    manifest["agents"][0]["skills"] = json!([]);
    manifest["agents"][1] = json!({"key": "ops", "name": "Ops",
        "skills": ["agents/ops/skills/a", "agents/ops/skills/b"], "files": "agents/ops/files"});
    manifest["channels"][1] = json!({"key": "again", "name": "Again", "members": ["research", "sales"]});
    let manifest = manifest.to_string();
    let path = zip(&[
        ("codync-import.json", manifest.as_bytes()),
        ("agents/research/files/notes.md", b"notes"),
        ("agents/ops/files/x.md", b"x"),
        ("agents/ops/skills/a/SKILL.md", b"---\nname: A\n---\nDo A."),
        ("agents/ops/skills/b/SKILL.md", b"\xff\xfe"),
    ]);
    let done = super::apply(&hub, &json!({"path": path})).await.unwrap();
    std::fs::remove_file(path).unwrap();

    let names: Vec<&str> = done["bots"].as_array().unwrap().iter().map(|b| b["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Research", "Sales"], "Ops fails on its second skill: {done}");
    let research = &done["bots"][0];
    assert_eq!((research["files"].as_u64(), research["memories"].as_u64()), (Some(1), Some(1)));
    let cwd = hub.store.bot(research["id"].as_str().unwrap()).unwrap().unwrap().config.cwd;
    assert_eq!(std::fs::read_to_string(std::path::Path::new(&cwd).join("imported/notes.md")).unwrap(), "notes");
    let skills = crate::market::list_skills(&hub.store);
    assert_eq!(skills["items"].as_array().unwrap().len(), 0, "Ops' first skill goes with it: {skills}");
    let groups = done["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1, "two groups of the same bots are one: {done}");
    let lead = hub.store.bot(groups[0]["id"].as_str().unwrap()).unwrap().unwrap().config.members[0].clone();
    assert_eq!(lead, research["id"].as_str().unwrap(), "the export's lead leads");
    let warnings = done["warnings"].to_string();
    assert!(warnings.contains("Couldn't import Ops") && warnings.contains("Again"), "{warnings}");
    // Tasks the hub spawned can still hold the database open, which Windows won't delete;
    // the folder is in the temp dir, so a failed cleanup doesn't fail the test.
    drop(hub);
    let _ = std::fs::remove_dir_all(dir);
}
