//! The card that asks the user to sign in to a bot's own agent (Claude Code…): posted when a
//! new bot's agent isn't signed in, or when a turn fails for that reason (also when a fallback agent
//! takes the turn over, then holding no message), and completed once it is.
//! Only agents Codync knows by name (`backends::HARNESSES`) get one; others show the agent's error.
use super::requests::{MAX_AGE, RequestKind, RequestStatus};
use crate::{
    LockExt,
    agent::{accounts, backends, bot::Cmd},
    hub::Hub,
    store::{BotConfig, EntryKind, Lane, now_ms},
};
use anyhow::{Result, anyhow};
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
    let Some(h) = backends::harness(&cfg.backend) else { return };
    let account = accounts::of_bot(&hub.store, &cfg.backend, cfg).map(str::to_owned);
    if account.is_none() && backends::signed_in(&cfg.backend) == Some(true) {
        return;
    }
    let (hub, bot, backend) = (hub.clone(), cfg.id.clone(), cfg.backend.clone());
    tokio::spawn(async move {
        let asked = match check(&hub, &backend, account.as_deref()).await {
            Ok(Some(false)) => invite_saying(&hub, &bot, &Lane::main(&bot), &backend, h.name, &[], BEFORE_FIRST),
            // Other chats waiting on this login go on.
            Ok(Some(true)) => signed_in(&hub, &backend, account.as_deref()),
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

/// What the agent's own check says about the sign-in of `account`, or of the computer's own login (`None`:
/// can't tell).
#[cfg(not(test))]
async fn check(hub: &Hub, backend: &str, account: Option<&str>) -> Result<Option<bool>> {
    Ok(match account {
        Some(id) => Some(accounts::check(&hub.store, id).await?.signed_in),
        None => crate::agent::auth::check(&hub.store, backend).await?["signedIn"].as_bool(),
    })
}
#[cfg(test)]
use tests::check;

/// Re-checks every installed agent's sign-in; chats waiting on one that's signed in now go on.
pub async fn refresh(hub: &Hub) {
    backends::refresh_sign_in().await;
    for h in backends::HARNESSES.iter().filter(|h| backends::signed_in(h.id) == Some(true)) {
        if let Err(error) = agent_signed_in(hub, h.id) {
            tracing::warn!(backend = h.id, error = format!("{error:#}"), "couldn't complete the sign-in cards");
        }
    }
}

/// `bot`'s turn failed because agent `backend` (`name`) needs signing in: a card in `lane` asks
/// the user to, and `entries` (the user messages it couldn't answer) go out again once they have.
/// A bot keeps one such card in each chat; messages that fail later join it.
pub fn invite(hub: &Hub, bot: &str, lane: &Lane, backend: &str, name: &str, entries: &[String]) -> Result<()> {
    // A routine's, a room's or another bot's turn has no message of the user's to send again.
    let text = if entries.is_empty() { NEEDED } else { HOLDING };
    invite_saying(hub, bot, lane, backend, name, entries, text)
}

/// The card, saying `text` when it's new.
fn invite_saying(
    hub: &Hub,
    bot: &str,
    lane: &Lane,
    backend: &str,
    name: &str,
    entries: &[String],
    text: &str,
) -> Result<()> {
    let _guard = SIGN_IN.locked();
    // In a room the card's chat is the room's: `botId` says whose it is.
    let mine = |card: &crate::store::Entry| card.bot_id == lane.chat && card.data["connectionRequest"]["botId"] == bot;
    if let Some(card) = hub.store.pending_sign_ins(backend)?.into_iter().find(mine) {
        let mut data = card.data;
        let mut ids = data["connectionRequest"]["entryIds"].as_array().cloned().unwrap_or_default();
        let held = ids.len();
        for id in entries {
            if !ids.iter().any(|v| v == id.as_str()) {
                ids.push(json!(id));
            }
        }
        if ids.len() == held {
            return Ok(());
        }
        data["text"] = HOLDING.into();
        data["connectionRequest"]["entryIds"] = Value::Array(ids);
        hub.set_entry(&card.id, &data).ok_or_else(|| anyhow!("Could not update the sign-in request"))?;
        return Ok(());
    }
    let mut request = json!({"kind": RequestKind::Agent, "status": RequestStatus::Pending, "title": name,
        "botId": bot, "backend": backend, "entryIds": entries});
    // The login to sign in is the bot's agent account's: the apps open that sign-in, not the computer's own.
    let cfg = hub.store.bot(bot)?.map(|b| b.config);
    if let Some(account) = cfg.as_ref().and_then(|cfg| accounts::of_bot(&hub.store, backend, cfg)) {
        request["account"] = account.into();
    }
    hub.add_entry(
        lane,
        EntryKind::Notice,
        hub.store.max_turn(bot),
        &json!({"text": text, "connectionRequest": request}),
    )
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
    let account = accounts::of_bot(&hub.store, backend, cfg);
    let news = if let Some(id) = account {
        accounts::note_signed_in(&hub.store, id, true)
    } else {
        // A prompt got through, so an earlier auth refusal is over.
        backends::clear_prompt_refused(backend);
        let known = backends::signed_in(backend) == Some(true);
        backends::set_signed_in(backend, Some(true));
        !known
    };
    if news && let Err(error) = signed_in(hub, backend, account) {
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
pub async fn confirm(hub: &Hub, bot: &str, backend: &str) -> Result<bool> {
    let cfg = hub.store.bot(bot)?.map(|b| b.config);
    let account = cfg.as_ref().and_then(|cfg| accounts::of_bot(&hub.store, backend, cfg));
    if check(hub, backend, account).await? != Some(true) {
        return Ok(false);
    }
    signed_in(hub, backend, account)?;
    Ok(true)
}

/// Checks agent account `id`'s sign-in for the apps: `{signedIn, account}`. Chats that asked the user to sign it
/// in go on once it is.
pub async fn check_account(hub: &Hub, id: &str) -> Result<Value> {
    let account = accounts::check(&hub.store, id).await?;
    if account.signed_in {
        signed_in(hub, &account.backend, Some(id))?;
    }
    Ok(account.status())
}

/// `backend` is signed in now on `account`, or on the computer's own login: the cards of the bots on that login
/// complete.
pub fn signed_in(hub: &Hub, backend: &str, account: Option<&str>) -> Result<()> {
    crate::agent::fallback::signed_in(hub, backend, account);
    let _guard = SIGN_IN.locked();
    for card in hub.store.pending_sign_ins(backend)? {
        let asker = card.data["connectionRequest"]["botId"].as_str().unwrap_or(&card.bot_id);
        let cfg = hub.store.bot(asker)?.map(|b| b.config);
        if cfg.as_ref().and_then(|cfg| accounts::of_bot(&hub.store, backend, cfg)) != account {
            continue;
        }
        let mut data = card.data.clone();
        data["connectionRequest"]["status"] = serde_json::to_value(RequestStatus::Ready)?;
        hub.set_entry(&card.id, &data).ok_or_else(|| anyhow!("Could not complete the sign-in request"))?;
        let ids = data["connectionRequest"]["entryIds"].as_array().cloned().unwrap_or_default();
        send_again(hub, &card.bot_id, ids.iter().filter_map(Value::as_str))?;
    }
    Ok(())
}

/// `bot` answers the user messages `ids` it couldn't answer (a message older than a day doesn't go out).
pub fn send_again<'a>(hub: &Hub, bot: &str, ids: impl IntoIterator<Item = &'a str>) -> Result<()> {
    let Some(bot) = hub.store.bot(bot)?.filter(|b| !b.deleted) else { return Ok(()) };
    let held = ids.into_iter().filter_map(|id| hub.store.entry(id));
    for mut entry in held.filter(|entry| now_ms() - entry.created_at <= MAX_AGE) {
        let prompt = crate::chat::prompt::of_entry(hub, &bot.config, &entry.data);
        entry.data["status"] = "queued".into();
        hub.set_entry(&entry.id, &entry.data);
        let lane = Lane::main(&entry.bot_id);
        hub.send_cmd(&bot.config.id, Cmd::Send { lane, entry_id: entry.id, text: prompt })?;
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
    pub(super) async fn check(_: &Hub, backend: &str, _account: Option<&str>) -> Result<Option<bool>> {
        Ok(CHECKS.locked().iter().find(|(b, _)| *b == backend).map(|(_, ok)| *ok))
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
        use crate::agent::fallback::{Reason, rest};
        let (hub, dir) = fixture();
        let wrapped = rest::account_of("kiro", Some("kiro-cli acp"));
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
        invite_saying(&hub, "bot", &lane, "fixture-first", "Fixture", &[], BEFORE_FIRST).unwrap();
        let sent = message(&hub, &lane, "hi");
        invite(&hub, "bot", &lane, "fixture-first", "Fixture", std::slice::from_ref(&sent)).unwrap();
        let cards = cards(&hub, "fixture-first", "bot");
        assert_eq!(cards.len(), 1, "the failed message joins the first card");
        assert_eq!(cards[0].data["connectionRequest"]["entryIds"], json!([sent]));
        assert_eq!(cards[0].data["text"], HOLDING);
        invite(&hub, "other", &Lane::main("other"), "fixture-first", "Fixture", &[]).unwrap();
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
        invite(&hub, "bot", &lane, "fixture", "Fixture", std::slice::from_ref(&first)).unwrap();
        invite(&hub, "bot", &lane, "fixture", "Fixture", &[second.clone(), first.clone()]).unwrap();
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
        invite_saying(&hub, "bot", &lane, "fixture-old", "Fixture", &[], BEFORE_FIRST).unwrap();
        let card = cards(&hub, "fixture-old", "bot")[0].id.clone();
        let day_ago = now_ms() - MAX_AGE - 1;
        hub.store.backdate_entry(&card, day_ago);
        let (stale, fresh) = (message(&hub, &lane, "stale"), message(&hub, &lane, "fresh"));
        hub.store.backdate_entry(&stale, day_ago);
        invite(&hub, "bot", &lane, "fixture-old", "Fixture", &[stale.clone(), fresh.clone()]).unwrap();
        agent_signed_in(&hub, "fixture-old").unwrap();
        assert_eq!(hub.store.entry(&fresh).unwrap().data["status"], "queued", "a day-old card still sends");
        assert_eq!(hub.store.entry(&stale).unwrap().data["status"], "sent", "a day-old message doesn't");
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn an_accounts_sign_in_completes_only_the_cards_of_the_bots_on_it() {
        use crate::agent::fallback::{Reason, rest};
        let (hub, dir) = fixture();
        accounts::tests::insert(&hub.store, &[("a1", "claude")]);
        let bot = |account: Value| {
            let cfg = json!({"id": "", "name": "claude", "backend": "claude", "cwd": dir, "account": account});
            hub.create_bot(serde_json::from_value(cfg).unwrap()).unwrap()["id"].as_str().unwrap().to_owned()
        };
        let (own, other) = (bot(Value::Null), bot(json!("a1")));
        for id in [&own, &other] {
            invite(&hub, id, &Lane::main(id), "claude", "Claude Code", &[]).unwrap();
        }
        let request = |bot: &str| cards(&hub, "claude", bot)[0].data["connectionRequest"].clone();
        assert_eq!(request(&other)["account"], "a1", "the card says which login to sign in");
        assert!(request(&own).get("account").is_none());
        // Each login rests by itself: the account's usage limit and sign-out aren't the computer's own login's.
        let config = |bot: &str| hub.store.bot(bot).unwrap().unwrap().config;
        let (on_own, on_account) = (rest::account(&config(&own)), rest::account(&config(&other)));
        let resting = |account: &str| rest::get(&hub.store, account, now_ms()).map(|r| r.reason);
        rest::record(&hub.store, &on_account, Reason::UsageLimit, now_ms(), None);
        assert_eq!(resting(&on_own), None, "the account's usage limit");
        rest::record(&hub.store, &on_account, Reason::SignedOut, now_ms(), None);
        rest::record(&hub.store, &on_own, Reason::SignedOut, now_ms(), None);
        signed_in(&hub, "claude", Some("a1")).unwrap();
        assert_eq!((resting(&on_account), resting(&on_own)), (None, Some(Reason::SignedOut)));
        assert!(cards(&hub, "claude", &other).is_empty(), "the account's bot goes on");
        assert_eq!(cards(&hub, "claude", &own).len(), 1, "the computer's own login is still signed out");
        rest::record(&hub.store, &on_account, Reason::SignedOut, now_ms(), None);
        agent_signed_in(&hub, "claude").unwrap();
        assert_eq!((resting(&on_account), resting(&on_own)), (Some(Reason::SignedOut), None));
        assert!(cards(&hub, "claude", &own).is_empty());
        // A prompt the account answered is news about the account alone.
        accounts::note_signed_in(&hub.store, "a1", false);
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
        invite(&hub, "bot", &lane, "fixture-known", "Fixture", &[]).unwrap();
        let on = |backend: &str| {
            serde_json::from_value::<BotConfig>(json!({"id": "bot", "name": "bot", "backend": backend})).unwrap()
        };
        answered(&hub, &on("fixture-known"));
        assert_eq!(cards(&hub, "fixture-known", "bot").len(), 1, "an agent Codync doesn't know has none");
        // A bot of its own on a fixture agent that answers, under a harness id no other test uses.
        let agent = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/team_agent.py");
        let cfg = json!({"id": "", "name": "elsewhere", "backend": "junie", "cwd": dir,
            "command": crate::shell::python(&agent)});
        let bot = hub.create_bot(serde_json::from_value(cfg).unwrap()).unwrap()["id"].as_str().unwrap().to_owned();
        // After a host restart nothing says signed out any more.
        backends::set_signed_in("junie", None);
        let lane = Lane::main(&bot);
        invite(&hub, &bot, &lane, "junie", "Junie", &[]).unwrap();
        let hi = message(&hub, &lane, "hi");
        hub.send_cmd(&bot, Cmd::Send { lane, entry_id: hi, text: "hi".into() }).unwrap();
        until("the answered turn to complete the card", || cards(&hub, "junie", &bot).is_empty()).await;
        assert_eq!(backends::signed_in("junie"), Some(true));
        hub.shutdown().await;
        let _ = std::fs::remove_dir_all(dir);
    }
}
