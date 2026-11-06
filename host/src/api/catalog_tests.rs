//! Team installation creates a complete group or removes its partially created bots.

use super::*;
use crate::LockExt;
use crate::remote::identity::Identity;
use crate::store::Store;
use std::path::PathBuf;

struct Fixture {
    hub: Arc<Hub>,
    dir: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("codync-catalog-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        // Keep connector defaults independent of the person's credential manager.
        *store.secret_key.locked() = Some(zeroize::Zeroizing::new(vec![0; 32]));
        let hub = Hub::new(store, "catalog-test".into(), Identity::load_or_create(&dir).unwrap(), "test".into(), 0);
        Self { hub, dir }
    }

    async fn install(&self) -> Result<Value> {
        install_team(&self.hub, catalog::team("software-team")?, Some("claude"), Some("test-model")).await
    }

    fn reject_insert(&self, group: bool) {
        let sql = if group {
            "CREATE TRIGGER reject_group BEFORE INSERT ON bots
             WHEN json_extract(NEW.config, '$.kind') = 'group'
             BEGIN SELECT RAISE(FAIL, 'injected group failure'); END;"
        } else {
            "CREATE TRIGGER reject_member BEFORE INSERT ON bots
             WHEN (SELECT COUNT(*) FROM bots) = 1
             BEGIN SELECT RAISE(FAIL, 'injected member failure'); END;"
        };
        self.hub
            .store
            .with_connection(|db| {
                db.execute_batch(sql)?;
                Ok(())
            })
            .unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[tokio::test]
async fn installing_a_team_creates_its_profiles_and_group_led_by_the_coordinator() {
    let fixture = Fixture::new();
    let team = catalog::team("software-team").unwrap();
    let profiles = team.profiles().unwrap();
    let installed = fixture.install().await.unwrap();
    let bots = fixture.hub.store.bots().unwrap();
    assert_eq!(bots.len(), profiles.len() + 1);
    assert!(bots.iter().all(|bot| !bot.deleted));

    let mut members = Vec::new();
    for profile in profiles {
        let bot = &bots.iter().find(|bot| bot.config.name == profile.name).unwrap().config;
        assert!(!bot.is_group());
        assert_eq!(bot.description, profile.instructions);
        assert_eq!(bot.backend, "claude");
        assert_eq!(bot.model.as_deref(), Some("test-model"));
        assert!(!bot.manage_team);
        members.push(bot.id.clone());
    }
    let group = &bots.iter().find(|bot| bot.config.is_group()).unwrap().config;
    assert_eq!(installed["bot"]["id"], group.id);
    assert_eq!(group.name, team.name);
    assert_eq!(group.description, team.purpose);
    assert_eq!(group.members, members);
    assert_eq!(group.chief_bot_id.as_ref(), members.first());
}

#[tokio::test]
async fn a_failed_member_insert_removes_the_coordinator_already_created() {
    let fixture = Fixture::new();
    fixture.reject_insert(false);
    let error = fixture.install().await.unwrap_err();
    assert!(format!("{error:#}").contains("injected member failure"));
    let bots = fixture.hub.store.bots().unwrap();
    assert_eq!(bots.len(), 1, "the coordinator was persisted before the member failed");
    assert!(bots.iter().all(|bot| bot.deleted), "no partial team remains live");
}

#[tokio::test]
async fn a_failed_group_insert_removes_the_coordinator_and_every_teammate() {
    let fixture = Fixture::new();
    fixture.reject_insert(true);
    let error = fixture.install().await.unwrap_err();
    assert!(format!("{error:#}").contains("injected group failure"));
    let bots = fixture.hub.store.bots().unwrap();
    assert_eq!(bots.len(), catalog::team("software-team").unwrap().profiles().unwrap().len());
    assert!(bots.iter().all(|bot| bot.deleted), "all profiles are removed when their group cannot be created");
}

#[test]
fn a_catalog_schedule_becomes_a_cron_trigger_on_this_clock() {
    let weekdays = routine_triggers(&serde_json::json!({"kind": "weekdays", "time": "09:05"})).unwrap();
    assert_eq!(weekdays[0]["expression"], "5 9 * * 1-5");
    let monthly = routine_triggers(&serde_json::json!({"kind": "monthly", "day": 3, "time": "08:00"})).unwrap();
    assert_eq!(monthly[0]["expression"], "0 8 3 * *");
    assert_eq!(monthly[0]["type"], "cron");
    assert!(routine_triggers(&serde_json::json!({"kind": "hourly"})).is_err());
}

#[tokio::test]
async fn installing_an_agent_brings_its_look_skills_and_routines() {
    let fixture = Fixture::new();
    let installed = install_agent(&fixture.hub, "qa-tester", Some("claude"), Some("test-model")).await.unwrap();
    let bot = fixture.hub.store.bot(installed["bot"]["id"].as_str().unwrap()).unwrap().unwrap().config;
    let agent = openbot::agent("qa-tester").unwrap();
    assert_eq!(bot.name, agent.name);
    assert_eq!(bot.description, agent.instructions);
    assert_eq!(bot.avatar_color, agent.avatar_color);
    assert_eq!(bot.catalog.as_deref(), Some("qa-tester"));
    assert_eq!(bot.skills.len(), agent.skills.len());
    for id in &bot.skills {
        assert!(openbot::skill(id).is_some(), "{id} came from the catalog");
    }
    let routines = fixture.hub.routines.list(&bot.id);
    assert_eq!(routines["routines"].as_array().unwrap().len(), agent.routines.len());
    assert_eq!(installed_agents(&fixture.hub).unwrap(), vec!["qa-tester"]);
}
