//! The card that asks the user to sign in to a bot's own agent (Claude Code…): posted when a
//! new bot's agent isn't signed in, or when a turn fails for that reason (also when a fallback agent
//! takes the turn over, then holding no message), and completed once it is.
//! Only agents Codync knows by name (`backends::HARNESSES`) get one; others show the agent's error.
use super::requests::{MAX_AGE, RequestKind, RequestStatus};
use crate::{
    LockExt,
    agent::{
        bot::Cmd,
        login_profiles::{self, Login},
    },
    chat::work::TaskMark,
    hub::Hub,
    store::{BotConfig, EntryKind, Lane, now_ms},
};
use anyhow::{Result, anyhow};
use codync_agent::backends;
use serde_json::{Value, json};
use std::sync::Arc;

/// Agent sign-in cards change from bot actors and API calls alike (no `.await` while held).
static SIGN_IN: std::sync::Mutex<()> = std::sync::Mutex::new(());

// The card's title names the agent.
const BEFORE_FIRST: &str = "Needs you to sign in before your first message.";
const NEEDED: &str = "Needs you to sign in.";
const HOLDING: &str = "Needs you to sign in. Your message goes out again once you're signed in.";

/// A new bot whose agent isn't signed in shows its sign-in card right away, so the user signs in
/// before writing instead of after the first message fails. The agent's own check runs in the
/// background.
pub fn ask_before_first_message(hub: &Arc<Hub>, cfg: &BotConfig) {
    if backends::harness(&cfg.backend).is_none() {
        return;
    }
    let login = Login::of(&hub.store, cfg);
    if login.profile.is_none() && backends::signed_in(&cfg.backend) == Some(true) {
        return;
    }
    let (hub, bot, backend) = (hub.clone(), cfg.id.clone(), cfg.backend.clone());
    tokio::spawn(async move {
        let asked = match check(&hub, &backend, login.profile.as_deref()).await {
            Ok(Some(false)) => invite_saying(&hub, &bot, &Lane::main(&bot), &login, &[], None, BEFORE_FIRST),
            // Other chats waiting on this login go on.
            Ok(Some(true)) => signed_in(&hub, &backend, login.profile.as_deref()),
            Ok(None) => Ok(()),
            Err(error) => {
                tracing::info!(bot, backend, error = format!("{error:#}"), "sign-in check failed");
                Ok(())
            }
        };
        if let Err(error) = asked {
            tracing::warn!(bot, error = format!("{error:#}"), "couldn't ask to sign in");
        }
    });
}

/// What the agent's own check says about the sign-in of `login_profile`, or of the computer's own login (`None`:
/// can't tell).
#[cfg(not(test))]
async fn check(hub: &Hub, backend: &str, login_profile: Option<&str>) -> Result<Option<bool>> {
    Ok(match login_profile {
        Some(id) => Some(login_profiles::check(&hub.store, id).await?.account.signed_in),
        None => crate::agent::auth::check(&hub.store, backend).await?["signedIn"].as_bool(),
    })
}
#[cfg(test)]
use tests::check;

/// Re-checks every installed agent's sign-in, and who each is signed in as; chats waiting on one that's signed
/// in now go on.
pub async fn refresh(hub: &Hub) {
    backends::refresh_sign_in().await;
    login_profiles::refresh_default().await;
    // The Usage screens call each default login by who it signs in.
    crate::usage::login_profiles_changed(hub);
    for h in backends::HARNESSES.iter().filter(|h| backends::signed_in(h.id) == Some(true)) {
        if let Err(error) = agent_signed_in(hub, h.id) {
            tracing::warn!(backend = h.id, error = format!("{error:#}"), "couldn't complete the sign-in cards");
        }
    }
}

/// `bot`'s turn failed because `login` needs signing in: a card in `lane` asks the user to, and `entries` (the messages
/// it couldn't answer) go out again once they have, each to the chat it was written in. A bot keeps one such card per
/// login in each chat; messages that fail later join it. A background thread's card is in its bot's chat, marked as its
/// task's (`task`), and kept apart from the bot's own.
pub fn invite(
    hub: &Hub,
    bot: &str,
    lane: &Lane,
    login: &Login,
    entries: &[String],
    task: Option<&TaskMark>,
) -> Result<()> {
    // A routine's, a room's or another bot's turn has no message of the user's to send again.
    let text = if entries.is_empty() { NEEDED } else { HOLDING };
    invite_saying(hub, bot, lane, login, entries, task, text)
}

/// The card, saying `text` when it's new. Its title names the agent.
fn invite_saying(
    hub: &Hub,
    bot: &str,
    lane: &Lane,
    login: &Login,
    entries: &[String],
    task: Option<&TaskMark>,
    text: &str,
) -> Result<()> {
    let _guard = SIGN_IN.locked();
    let backend = login.backend.as_str();
    // In a room the card's chat is the room's: `botId` says whose it is.
    let mine = |card: &crate::store::Entry| {
        let request = &card.data["connectionRequest"];
        card.bot_id == lane.chat
            && request["botId"] == bot
            && request["loginProfile"].as_str() == login.profile.as_deref()
            && card.data["thread"]["id"].as_str() == task.map(TaskMark::thread)
    };
    if let Some(card) = hub.store.pending_sign_ins(backend)?.into_iter().find(mine) {
        let mut data = card.data.clone();
        let mut ids = data["connectionRequest"]["entryIds"].as_array().cloned().unwrap_or_default();
        let held = ids.len();
        for id in entries {
            if !ids.iter().any(|v| v == id.as_str()) {
                ids.push(json!(id));
            }
        }
        if ids.len() > held {
            data["text"] = HOLDING.into();
            data["connectionRequest"]["entryIds"] = Value::Array(ids);
        }
        // A thread's card is the task's it works on now (the next task joins the thread).
        if let Some(task) = task {
            task.apply(&mut data);
        }
        if data != card.data {
            hub.set_entry(&card.id, &data).ok_or_else(|| anyhow!("Could not update the sign-in request"))?;
        }
        return Ok(());
    }
    let name = backends::harness(backend).map_or(backend, |h| h.name);
    let mut request = json!({"kind": RequestKind::Agent, "status": RequestStatus::Pending, "title": name,
        "botId": bot, "backend": backend, "entryIds": entries});
    // A login profile's: the apps open that sign-in, not the computer's own.
    if let Some(profile) = &login.profile {
        request["loginProfile"] = profile.as_str().into();
    }
    let mut data = json!({"text": text, "connectionRequest": request});
    if let Some(task) = task {
        task.apply(&mut data);
    }
    hub.add_entry(lane, EntryKind::Notice, hub.store.max_turn(bot), &data)
        .ok_or_else(|| anyhow!("Could not save the sign-in request"))?;
    Ok(())
}

/// A turn of `cfg`'s agent just finished, so the login it ran on is signed in whatever Codync last found (the
/// user signed in outside Codync, or since the host started): its cards complete.
pub fn answered(hub: &Hub, cfg: &BotConfig) {
    let backend = cfg.backend.as_str();
    if backends::harness(backend).is_none() {
        return;
    }
    let profile = login_profiles::of_bot(&hub.store, backend, cfg);
    let news = if let Some(id) = profile {
        login_profiles::note_signed_in(&hub.store, id, true)
    } else {
        // A prompt got through, so an earlier auth refusal is over.
        backends::clear_prompt_refused(backend);
        let known = backends::signed_in(backend) == Some(true);
        backends::set_signed_in(backend, Some(true));
        !known
    };
    if news && let Err(error) = signed_in(hub, backend, profile) {
        tracing::warn!(backend, error = format!("{error:#}"), "couldn't complete the sign-in cards");
    }
}

/// Agent `backend` is signed in now on the computer's own login: the sign-in cards of the bots on it complete,
/// and the messages they hold go out again (a message older than a day doesn't).
pub fn agent_signed_in(hub: &Hub, backend: &str) -> Result<()> {
    signed_in(hub, backend, None)
}

/// The card `bot` got for agent `backend` is answered: whether the login it waits on is signed in now, which
/// completes that login's cards.
pub async fn confirm(hub: &Hub, backend: &str, login_profile: Option<&str>) -> Result<bool> {
    if check(hub, backend, login_profile).await? != Some(true) {
        return Ok(false);
    }
    signed_in(hub, backend, login_profile)?;
    Ok(true)
}

/// Checks login profile `id`'s sign-in for the apps: `{signedIn, loginProfile}`. Chats that asked the user to
/// sign it in go on once it is.
pub async fn check_login_profile(hub: &Hub, id: &str) -> Result<Value> {
    settled(hub, &login_profiles::check(&hub.store, id).await?)
}

/// Gives API key login profile `id` its keys (`login_profiles::keys::set`), and answers as
/// [`check_login_profile`] does.
pub async fn set_keys(hub: &Hub, id: &str, typed: &serde_json::Map<String, Value>, import: &[String]) -> Result<Value> {
    settled(hub, &login_profiles::keys::set(&hub.store, id, typed, import).await?)
}

/// `profile` as just checked or changed: once it's signed in, the cards that wait on it complete.
fn settled(hub: &Hub, profile: &login_profiles::Profile) -> Result<Value> {
    if profile.account.signed_in {
        signed_in(hub, &profile.account.agent, Some(&profile.account.id))?;
    } else {
        // Signed out, it leaves the Usage screens.
        crate::usage::login_profiles_changed(hub);
    }
    Ok(profile.status())
}

/// `backend` is signed in now on `login_profile`, or on the computer's own login: the cards of the bots on that
/// login complete.
pub fn signed_in(hub: &Hub, backend: &str, login_profile: Option<&str>) -> Result<()> {
    crate::agent::fallback::signed_in(hub, backend, login_profile);
    crate::agent::auth::model_list_signed_in(&hub.store, backend, login_profile);
    if login_profile.is_some() {
        // Its limits are read and listed from now on.
        crate::usage::login_profiles_changed(hub);
    }
    let _guard = SIGN_IN.locked();
    for card in hub.store.pending_sign_ins(backend)? {
        if card.data["connectionRequest"]["loginProfile"].as_str() != login_profile {
            continue;
        }
        let mut data = card.data.clone();
        data["connectionRequest"]["status"] = serde_json::to_value(RequestStatus::Ready)?;
        hub.set_entry(&card.id, &data).ok_or_else(|| anyhow!("Could not complete the sign-in request"))?;
        let ids = data["connectionRequest"]["entryIds"].as_array().cloned().unwrap_or_default();
        // One card's messages that can't go out don't keep the next card's from going.
        if let Err(error) = send_again(hub, ids.iter().filter_map(Value::as_str)) {
            tracing::warn!(card = card.id, error = format!("{error:#}"), "couldn't send held messages again");
        }
    }
    Ok(())
}

/// Signs account login profile `id` in through its agent itself (a browser on this computer), then checks it as
/// [`check_login_profile`] does. An API key profile has no such sign-in.
pub async fn sign_in_account(hub: &Hub, id: &str) -> Result<Value> {
    let backend = login_profiles::find_account(&hub.store, id)?.agent;
    // The login profile isn't removed under its sign-in.
    let _running = login_profiles::Running::on(id);
    crate::agent::auth::authenticate_account(&hub.store, &backend, id).await?;
    // A finished sign-in counts again, whatever a prompt was refused for before.
    backends::clear_prompt_refused(id);
    check_login_profile(hub, id).await
}

/// The messages `ids` that couldn't be answered go out again, each to the chat it was written in: a bot's own, or
/// a background thread's (its task's message, while the thread still works on that task, however long it waited). A
/// bot's own message older than a day doesn't, and nor does one whose chat is gone.
pub fn send_again<'a>(hub: &Hub, ids: impl IntoIterator<Item = &'a str>) -> Result<()> {
    for mut entry in ids.into_iter().filter_map(|id| hub.store.entry(id)) {
        let Some(bot) = hub.store.bot(&entry.bot_id)?.filter(|b| !b.deleted) else { continue };
        let goes = if bot.config.is_thread() {
            // A thread whose task was stopped, or went back to the box, has nothing to answer it in.
            crate::chat::work::card_task(&hub.store, &bot.config).is_some()
        } else {
            now_ms() - entry.created_at <= MAX_AGE
        };
        if !goes {
            continue;
        }
        let prompt = crate::chat::prompt::of_entry(hub, &bot.config, &entry);
        entry.data["status"] = "queued".into();
        hub.set_entry(&entry.id, &entry.data);
        let lane = Lane::main(&entry.bot_id);
        let arrived = hub.pause.generation(bot.config.owner());
        hub.send_cmd(
            &bot.config.id,
            Cmd::Send {
                lane,
                entry_id: entry.id,
                text: prompt,
                arrived_epoch: arrived.epoch,
                arrived_blocked: arrived.blocked,
            },
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::requests::tests::fixture;
    use super::*;
    use std::time::Duration;

    /// Sign-in checks the tests answer themselves (no real agent starts): `(backend, signed in)`.
    static CHECKS: std::sync::Mutex<Vec<(&str, bool)>> = std::sync::Mutex::new(Vec::new());

    #[allow(clippy::unused_async)] // the real check's signature
    pub(super) async fn check(_: &Hub, backend: &str, _login_profile: Option<&str>) -> Result<Option<bool>> {
        Ok(CHECKS.locked().iter().find(|(b, _)| *b == backend).map(|(_, ok)| *ok))
    }

    /// The computer's own login of agent `backend`.
    fn own(backend: &str) -> Login {
        Login { backend: backend.to_owned(), profile: None }
    }

    fn cards(hub: &Hub, backend: &str, bot: &str) -> Vec<crate::store::Entry> {
        hub.store.pending_sign_ins(backend).unwrap().into_iter().filter(|c| c.bot_id == bot).collect()
    }

    fn create(hub: &Arc<Hub>, backend: &str) -> String {
        let cfg =
            json!({"id": "", "name": backend, "backend": backend, "cwd": std::env::temp_dir(), "command": "true"});
        let made = hub.create_bot(serde_json::from_value(cfg).unwrap()).unwrap();
        made["id"].as_str().unwrap().to_owned()
    }

    /// A message of the user's in `lane`, as sent.
    fn message(hub: &Hub, lane: &Lane, text: &str) -> String {
        hub.add_entry(lane, EntryKind::User, 1, &json!({"text": text, "status": "sent"})).unwrap().id
    }

    async fn until(what: &str, done: impl Fn() -> bool) {
        let waited = tokio::time::timeout(Duration::from_secs(20), async {
            while !done() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        });
        waited.await.unwrap_or_else(|_| panic!("timed out waiting for {what}"));
    }

    #[tokio::test]
    async fn signing_in_brings_bots_back_from_their_fallbacks() {
        use crate::agent::fallback::{Reason, login, rest};
        let (hub, dir) = fixture();
        let wrapped = login::key("kiro", Some("kiro-cli acp"), None);
        rest::record(&hub.store, &wrapped, Reason::SignedOut, now_ms(), None);
        rest::record(&hub.store, "kiro", Reason::UsageLimit, now_ms(), None);
        agent_signed_in(&hub, "kiro").unwrap();
        assert_eq!(rest::get(&hub.store, &wrapped, now_ms()), None, "the agent no longer rests for its sign-in");
        let usage = rest::get(&hub.store, "kiro", now_ms()).map(|r| r.reason);
        assert_eq!(usage, Some(Reason::UsageLimit), "signing in doesn't give usage back");
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_new_bot_asks_to_sign_in_only_when_its_agent_is_signed_out() {
        let (hub, dir) = fixture();
        // Harness ids no other test uses: sign-in state is per process.
        CHECKS.locked().extend([("kimi", false), ("qoder", false)]);
        backends::set_signed_in("qoder", Some(true));
        let known = create(&hub, "qoder");
        let unknown = create(&hub, "fixture-unknown");
        let signed_out = create(&hub, "kimi");
        until("the card", || !cards(&hub, "kimi", &signed_out).is_empty()).await;
        let card = &cards(&hub, "kimi", &signed_out)[0];
        assert_eq!(card.data["text"], BEFORE_FIRST);
        assert_eq!(card.data["connectionRequest"]["title"], "Kimi Code");
        assert!(cards(&hub, "qoder", &known).is_empty(), "a signed-in agent asks nothing");
        assert!(cards(&hub, "fixture-unknown", &unknown).is_empty(), "nor one Codync doesn't know");
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn the_first_card_holds_the_messages_that_fail_later() {
        let (hub, dir) = fixture();
        let lane = Lane::main("bot");
        invite_saying(&hub, "bot", &lane, &own("fixture-first"), &[], None, BEFORE_FIRST).unwrap();
        let sent = message(&hub, &lane, "hi");
        invite(&hub, "bot", &lane, &own("fixture-first"), std::slice::from_ref(&sent), None).unwrap();
        let cards = cards(&hub, "fixture-first", "bot");
        assert_eq!(cards.len(), 1, "the failed message joins the first card");
        assert_eq!(cards[0].data["connectionRequest"]["entryIds"], json!([sent]));
        assert_eq!(cards[0].data["text"], HOLDING);
        invite(&hub, "other", &Lane::main("other"), &own("fixture-first"), &[], None).unwrap();
        let routine = &hub.store.pending_sign_ins("fixture-first").unwrap()[1];
        assert_eq!(routine.data["text"], NEEDED, "a turn without the user's message promises none");
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn signing_in_completes_the_card_and_sends_its_messages_again() {
        let (hub, dir) = fixture();
        let lane = Lane::main("bot");
        let (first, second) = (message(&hub, &lane, "alpha-7"), message(&hub, &lane, "bravo-9"));
        invite(&hub, "bot", &lane, &own("fixture"), std::slice::from_ref(&first), None).unwrap();
        invite(&hub, "bot", &lane, &own("fixture"), &[second.clone(), first.clone()], None).unwrap();
        let cards = hub.store.pending_sign_ins("fixture").unwrap();
        assert_eq!(cards.len(), 1, "a bot keeps one sign-in card");
        assert_eq!(cards[0].data["connectionRequest"]["entryIds"], json!([first, second]));
        agent_signed_in(&hub, "fixture").unwrap();
        assert_eq!(hub.store.entry(&cards[0].id).unwrap().data["connectionRequest"]["status"], "ready");
        let prompts = dir.join("prompts.jsonl");
        let sent = || std::fs::read_to_string(&prompts).is_ok_and(|p| p.contains("alpha-7") && p.contains("bravo-9"));
        until("both messages to go out again", sent).await;
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn only_messages_younger_than_a_day_go_out_again_whatever_the_cards_age() {
        let (hub, dir) = fixture();
        let lane = Lane::main("bot");
        invite_saying(&hub, "bot", &lane, &own("fixture-old"), &[], None, BEFORE_FIRST).unwrap();
        let card = cards(&hub, "fixture-old", "bot")[0].id.clone();
        let day_ago = now_ms() - MAX_AGE - 1;
        hub.store.backdate_entry(&card, day_ago);
        let (stale, fresh) = (message(&hub, &lane, "stale"), message(&hub, &lane, "fresh"));
        hub.store.backdate_entry(&stale, day_ago);
        invite(&hub, "bot", &lane, &own("fixture-old"), &[stale.clone(), fresh.clone()], None).unwrap();
        agent_signed_in(&hub, "fixture-old").unwrap();
        assert_eq!(hub.store.entry(&fresh).unwrap().data["status"], "queued", "a day-old card still sends");
        assert_eq!(hub.store.entry(&stale).unwrap().data["status"], "sent", "a day-old message doesn't");
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_threads_message_goes_out_again_while_its_task_works_and_one_card_doesnt_hold_up_the_next() {
        let (hub, dir) = fixture();
        // Two threads of `bot`, neither running: `thread-1` still works on its task, `thread-2`'s task ended.
        for thread in ["thread-1", "thread-2"] {
            let cfg = json!({"id": thread, "name": "bot", "backend": "fixture-thread", "cwd": dir, "threadOf": "bot"});
            hub.store.save_bot(&serde_json::from_value(cfg).unwrap()).unwrap();
        }
        crate::chat::work::put_working_for_test(&hub.store, "bot", "thread-1");
        let held = |chat: &str, text: &str| {
            let lane = Lane::main(chat);
            let sent = message(&hub, &lane, text);
            invite(&hub, chat, &lane, &own("fixture-thread"), std::slice::from_ref(&sent), None).unwrap();
            sent
        };
        let (waited, stopped) = (held("thread-1", "waited"), held("thread-2", "stopped"));
        held("bot", "own-7");
        hub.store.backdate_entry(&waited, now_ms() - MAX_AGE - 1);
        agent_signed_in(&hub, "fixture-thread").unwrap();
        let status = |id: &str| hub.store.entry(id).unwrap().data["status"].clone();
        assert_eq!(status(&waited), "queued", "a task waits for its sign-in however long");
        assert_eq!(status(&stopped), "sent", "a thread whose task ended has nothing to answer it in");
        // `thread-1` has no actor to take its message: the bot's own card still sends.
        let prompts = dir.join("prompts.jsonl");
        until("the bot's message to go out again", || {
            std::fs::read_to_string(&prompts).is_ok_and(|p| p.contains("own-7"))
        })
        .await;
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_login_profiles_sign_in_completes_only_the_cards_of_the_bots_on_it() {
        use crate::agent::fallback::{Reason, login, rest};
        let (hub, dir) = fixture();
        login_profiles::tests::insert(&hub.store, &[("a1", "claude")]);
        let bot = |profile: Value| {
            let cfg = json!({"id": "", "name": "claude", "backend": "claude", "cwd": dir, "loginProfile": profile});
            hub.create_bot(serde_json::from_value(cfg).unwrap()).unwrap()["id"].as_str().unwrap().to_owned()
        };
        let (own, other) = (bot(Value::Null), bot(json!("a1")));
        for id in [&own, &other] {
            let login = Login::of(&hub.store, &hub.store.bot(id).unwrap().unwrap().config);
            invite(&hub, id, &Lane::main(id), &login, &[], None).unwrap();
        }
        let request = |bot: &str| cards(&hub, "claude", bot)[0].data["connectionRequest"].clone();
        assert_eq!(request(&other)["loginProfile"], "a1", "the card says which login to sign in");
        assert!(request(&own).get("loginProfile").is_none());
        // Each login rests by itself: the login profile's usage limit and sign-out aren't the computer's own login's.
        let config = |bot: &str| hub.store.bot(bot).unwrap().unwrap().config;
        let (on_own, on_profile) = (login::of(&config(&own)), login::of(&config(&other)));
        let resting = |login: &str| rest::get(&hub.store, login, now_ms()).map(|r| r.reason);
        rest::record(&hub.store, &on_profile, Reason::UsageLimit, now_ms(), None);
        assert_eq!(resting(&on_own), None, "the login profile's usage limit");
        rest::record(&hub.store, &on_profile, Reason::SignedOut, now_ms(), None);
        rest::record(&hub.store, &on_own, Reason::SignedOut, now_ms(), None);
        signed_in(&hub, "claude", Some("a1")).unwrap();
        assert_eq!((resting(&on_profile), resting(&on_own)), (None, Some(Reason::SignedOut)));
        assert!(cards(&hub, "claude", &other).is_empty(), "the login profile's bot goes on");
        assert_eq!(cards(&hub, "claude", &own).len(), 1, "the computer's own login is still signed out");
        rest::record(&hub.store, &on_profile, Reason::SignedOut, now_ms(), None);
        agent_signed_in(&hub, "claude").unwrap();
        assert_eq!((resting(&on_profile), resting(&on_own)), (Some(Reason::SignedOut), None));
        assert!(cards(&hub, "claude", &own).is_empty());
        // A prompt the login profile answered is news about it alone.
        login_profiles::note_signed_in(&hub.store, "a1", false);
        backends::set_signed_in("claude", Some(false));
        let cfg = hub.store.bot(&other).unwrap().unwrap().config;
        answered(&hub, &cfg);
        assert_eq!(backends::signed_in("claude"), Some(false));
        backends::set_signed_in("claude", None);
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn a_finished_turn_completes_the_card_of_an_agent_signed_in_elsewhere() {
        let (hub, dir) = fixture();
        let lane = Lane::main("bot");
        invite(&hub, "bot", &lane, &own("fixture-known"), &[], None).unwrap();
        let on = |backend: &str| {
            serde_json::from_value::<BotConfig>(json!({"id": "bot", "name": "bot", "backend": backend})).unwrap()
        };
        answered(&hub, &on("fixture-known"));
        assert_eq!(cards(&hub, "fixture-known", "bot").len(), 1, "an agent Codync doesn't know has none");
        // A bot of its own on a fixture agent that answers, under a harness id no other test uses.
        let agent = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        let cfg = json!({"id": "", "name": "elsewhere", "backend": "junie", "cwd": dir,
            "command": codync_agent::shell::python(&agent)});
        let bot = hub.create_bot(serde_json::from_value(cfg).unwrap()).unwrap()["id"].as_str().unwrap().to_owned();
        // After a host restart nothing says signed out any more.
        backends::set_signed_in("junie", None);
        let lane = Lane::main(&bot);
        invite(&hub, &bot, &lane, &own("junie"), &[], None).unwrap();
        let hi = message(&hub, &lane, "hi");
        let arrived = hub.pause.generation(&bot);
        hub.send_cmd(
            &bot,
            Cmd::Send {
                lane,
                entry_id: hi,
                text: "hi".into(),
                arrived_epoch: arrived.epoch,
                arrived_blocked: arrived.blocked,
            },
        )
        .unwrap();
        until("the answered turn to complete the card", || cards(&hub, "junie", &bot).is_empty()).await;
        assert_eq!(backends::signed_in("junie"), Some(true));
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
