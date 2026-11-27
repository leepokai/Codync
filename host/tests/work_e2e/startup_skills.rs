use crate::common::{bot, host::Host};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::path::Path;

const REJECT_PARENT: &str = "CREATE TRIGGER reject_skill_adoption BEFORE UPDATE OF config ON bots
    WHEN EXISTS (SELECT 1 FROM json_each(NEW.config, '$.skills') WHERE value = 'blocked')
    BEGIN SELECT RAISE(ABORT, 'fixture refused the parent config write'); END;";
const REJECT_LIST: &str = "CREATE TRIGGER reject_skill_adoption BEFORE INSERT ON kv
    WHEN NEW.k = 'skills' AND EXISTS (
        SELECT 1 FROM json_each(NEW.v) WHERE json_extract(value, '$.id') = 'blocked'
        AND json_extract(value, '$.writtenBy') <> 'thread-blocked'
    ) BEGIN SELECT RAISE(ABORT, 'fixture refused the skills list write'); END;";

fn seed_skills(home: &Path, parent: &str, ids: &[&str]) -> Connection {
    let db = Connection::open(home.join("codync.db")).unwrap();
    let skills: Vec<_> = ids
        .iter()
        .map(|id| {
            let thread = format!("thread-{id}");
            db.execute(
                "INSERT INTO parked_threads(id, bot, config)
                 SELECT ?2, id, json_set(config, '$.id', ?2, '$.threadOf', id) FROM bots WHERE id = ?1",
                params![parent, thread],
            )
            .unwrap();
            let dir = home.join("skills").join(id);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), "---\nname: Legacy\ndescription: Old.\n---\n\nOld.\n").unwrap();
            json!({"id": id, "name": id, "source": "custom", "writtenBy": thread})
        })
        .collect();
    db.execute("INSERT INTO kv(k, v) VALUES('skills', ?1)", [serde_json::to_string(&skills).unwrap()]).unwrap();
    db
}

fn assert_author(host: &Host, skill: &str, author: &str) {
    let db = Connection::open(host.home.join("codync.db")).unwrap();
    let saved: String = db
        .query_row(
            "SELECT json_extract(value, '$.writtenBy') FROM kv, json_each(v)
             WHERE k = 'skills' AND json_extract(value, '$.id') = ?1",
            [skill],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(saved, author, "saved author of {skill}");
}

fn rewrite(bot: &str) -> Value {
    json!({"botId": bot, "name": "save_skill", "arguments": {
        "id": "blocked", "name": "Blocked", "description": "Updated.", "instructions": "New."
    }})
}

async fn startup_retries_an_uncommitted_skill(trigger: &str) {
    let mut host = Host::start().await;
    let parent = host.bot("Parent", "ask").await;
    let other = host.bot("Other", "ask").await;
    host.restart(|home| seed_skills(home, &parent, &["blocked", "healthy"]).execute_batch(trigger).unwrap()).await;

    assert_eq!(bot(&host.snapshot().await, &parent)["skills"], json!(["healthy"]));
    assert_author(&host, "blocked", "thread-blocked");
    assert_author(&host, "healthy", &parent);
    assert!(host.refused("chatCall", rewrite(&parent)).await.contains("wasn't saved by you"));
    assert!(host.refused("chatCall", rewrite(&other)).await.contains("wasn't saved by you"));
    assert_eq!(host.call("updateBot", json!({"id": parent, "skills": []})).await["bot"]["skills"], json!([]));

    host.restart(|home| {
        Connection::open(home.join("codync.db")).unwrap().execute_batch("DROP TRIGGER reject_skill_adoption").unwrap();
    })
    .await;
    assert_eq!(bot(&host.snapshot().await, &parent)["skills"], json!(["blocked"]));
    assert_author(&host, "blocked", &parent);
    assert_author(&host, "healthy", &parent);
    assert!(host.refused("chatCall", rewrite(&other)).await.contains("wasn't saved by you"));
    assert_eq!(host.call("chatCall", rewrite(&parent)).await["skill"]["id"], "blocked");

    assert_eq!(host.call("updateBot", json!({"id": parent, "skills": []})).await["bot"]["skills"], json!([]));
    host.restart(|_| {}).await;
    assert_eq!(bot(&host.snapshot().await, &parent)["skills"], json!([]));
}

#[tokio::test]
async fn a_failed_parent_write_does_not_block_other_startup_skill_adoptions() {
    startup_retries_an_uncommitted_skill(REJECT_PARENT).await;
}

#[tokio::test]
async fn a_failed_skill_list_write_does_not_leave_the_parent_skill_enabled() {
    startup_retries_an_uncommitted_skill(REJECT_LIST).await;
}

#[tokio::test]
async fn unreadable_threads_do_not_block_other_startup_skill_adoptions() {
    let mut host = Host::start().await;
    let parent = host.bot("Parent", "ask").await;
    host.restart(|home| {
        seed_skills(home, &parent, &["parked", "active", "healthy"])
            .execute_batch(
                "UPDATE parked_threads SET config = 'invalid JSON' WHERE id = 'thread-parked';
                 INSERT INTO bots(id, rev, config) VALUES('thread-active', 0, X'00');
                 DELETE FROM parked_threads WHERE id = 'thread-active';",
            )
            .unwrap();
    })
    .await;
    assert_eq!(bot(&host.snapshot().await, &parent)["skills"], json!(["healthy"]));
    assert_author(&host, "parked", "thread-parked");
    assert_author(&host, "active", "thread-active");
    assert_author(&host, "healthy", &parent);
}
