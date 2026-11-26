use super::*;

fn store() -> Store {
    let dir = std::env::temp_dir().join(format!("codync-sessions-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    Store::open(&dir.join("t.db")).unwrap()
}

fn caller(kind: AgentKind, session: Option<&str>, cwd: &str) -> Caller {
    Caller { kind, session: session.map(str::to_owned), process: "7-a".into(), cwd: cwd.into() }
}

/// Hands out `base`, `base-2`… like the network's address book, remembering what it gave.
fn book() -> impl FnMut(&str) -> Result<String> {
    let mut given: Vec<String> = Vec::new();
    move |base: &str| {
        let free = crate::network::numbered(base, given.len(), |h| !given.iter().any(|g| g == h));
        given.push(free.clone());
        Ok(free)
    }
}

/// Registers `caller` as `agentCall` does, with the address it would start from.
fn register(
    sessions: &Sessions,
    store: &Store,
    caller: &Caller,
    book: &mut impl FnMut(&str) -> Result<String>,
) -> (Session, Registered) {
    let (info, base) = caller.look();
    sessions.register(store, caller, info.title, None, || book(&base)).unwrap()
}

#[test]
fn a_session_keeps_its_address_and_a_new_one_gets_a_number() {
    let (store, sessions, mut book) = (store(), Sessions::default(), book());
    let dir = std::env::temp_dir().join(format!("api-{}", uuid::Uuid::new_v4()));
    let cwd = dir.to_string_lossy().into_owned();
    let first = register(&sessions, &store, &caller(AgentKind::ClaudeCode, Some("s1"), &cwd), &mut book).0;
    assert!(first.address.ends_with("-claude") && !first.temporary, "{first:?}");
    assert_eq!(first.id(), Some("s1"));
    let again = register(&sessions, &store, &caller(AgentKind::ClaudeCode, Some("s1"), &cwd), &mut book).0;
    assert_eq!(again.address, first.address, "the same session id comes back to its address");
    let other = register(&sessions, &store, &caller(AgentKind::ClaudeCode, Some("s2"), &cwd), &mut book).0;
    assert_eq!(other.address, format!("{}-2", first.address));
    let codex = register(&sessions, &store, &caller(AgentKind::Codex, Some("s1"), &cwd), &mut book).0;
    assert!(codex.address.ends_with("-codex"), "the kind is part of the key: {codex:?}");
    let no_id = register(&sessions, &store, &caller(AgentKind::Other, None, "/"), &mut book).0;
    assert_eq!((no_id.address.as_str(), no_id.temporary, no_id.id()), ("agent", true, None));
    for s in [&first, &other, &codex, &no_id] {
        assert!(!s.address.contains(['@', '.']), "an address never holds what splits one: {s:?}");
    }
}

#[test]
fn mail_waits_for_its_session_and_follows_a_rename() {
    let (store, sessions, mut book) = (store(), Sessions::default(), book());
    let s = register(&sessions, &store, &caller(AgentKind::Codex, Some("t"), "/work/api"), &mut book).0;
    let mail = |n: usize| Mail {
        id: n.to_string(),
        from: "lead@laptop".into(),
        body: format!("letter {n}"),
        reply_to: None,
        at: 0,
    };
    for n in 0..MAIL_MAX + 2 {
        sessions.post(&store, &s.key, mail(n)).unwrap();
    }
    assert_eq!(sessions.unread(&store, &s.key), MAIL_MAX);
    let renamed = sessions.rename(&store, &s.key, "reviewer", || book("reviewer")).unwrap();
    assert_eq!(renamed.address, "reviewer");
    let again = sessions.rename(&store, &s.key, "reviewer", || book("reviewer")).unwrap();
    assert_eq!(again.address, "reviewer", "the same name again keeps its address");
    assert_eq!(sessions.by_address(&store, "Reviewer").map(|s| s.key), Some(s.key.clone()));
    let taken = sessions.take(&store, &s.key).unwrap();
    assert_eq!((taken.len(), taken[0].id.as_str()), (MAIL_MAX, "2"), "the oldest went first");
    assert_eq!(sessions.unread(&store, &s.key), 0, "read once taken");
    let back = register(&sessions, &store, &caller(AgentKind::Codex, Some("t"), "/elsewhere"), &mut book).0;
    assert_eq!(back.address, "reviewer", "the address never follows anything but set_name");
}

#[test]
fn a_session_unseen_for_long_is_forgotten_unless_mail_waits() {
    let (store, sessions, mut book) = (store(), Sessions::default(), book());
    let quiet = register(&sessions, &store, &caller(AgentKind::Codex, Some("quiet"), "/work/api"), &mut book).0;
    let waiting = register(&sessions, &store, &caller(AgentKind::Codex, Some("waiting"), "/work/api"), &mut book).0;
    let temporary = register(&sessions, &store, &caller(AgentKind::Other, None, "/"), &mut book).0;
    let mail = Mail { id: "1".into(), from: "lead@laptop".into(), body: "hi".into(), reply_to: None, at: 0 };
    sessions.post(&store, &waiting.key, mail.clone()).unwrap();
    sessions.post(&store, &temporary.key, mail).unwrap();
    let mut all = load(&store);
    for s in all.values_mut() {
        s.seen -= KEPT_FOR_MS;
    }
    save(&store, &all).unwrap();
    // Even a temporary newcomer's call that forgets them changes what's offered.
    let newcomer = Caller { process: "8-b".into(), ..caller(AgentKind::Other, None, "/") };
    let now = register(&sessions, &store, &newcomer, &mut book).1;
    assert!(now.new && now.offer, "{now:?}");
    let left: Vec<String> = sessions.all(&store).into_iter().map(|s| s.key).collect();
    assert!(!left.contains(&quiet.key) && !left.contains(&temporary.key), "{left:?}");
    assert!(left.contains(&waiting.key), "its mail keeps it: {left:?}");
    assert_eq!(sessions.unread(&store, &temporary.key), 0, "a forgotten session's mail goes with it");
}

#[test]
fn a_hook_tells_of_new_mail_once_and_never_registers() {
    let store = store();
    let sessions = Sessions::default();
    assert_eq!(sessions.hook(&store, "codex:unknown", Tier::Turn).unwrap(), None, "a hook doesn't register");
    let (s, _) = sessions
        .register(&store, &caller(AgentKind::Codex, Some("t1"), "/tmp"), None, None, || Ok("api-codex".into()))
        .unwrap();
    assert_eq!(s.key, session_key(AgentKind::Codex, "t1"));
    assert_eq!(s.tier, Tier::Pull);
    let letter = |id: &str| Mail { id: id.into(), from: "x".into(), body: "hi".into(), reply_to: None, at: 0 };
    assert_eq!(
        sessions.hook(&store, &s.key, Tier::Turn).unwrap(),
        Some(Hooked { unread: 0, newest: None, raised: true })
    );
    sessions.post(&store, &s.key, letter("1")).unwrap();
    let told = Some(Hooked { unread: 1, newest: Some("1".into()), raised: false });
    assert_eq!(sessions.hook(&store, &s.key, Tier::Turn).unwrap(), told);
    assert_eq!(
        sessions.hook(&store, &s.key, Tier::Turn).unwrap(),
        told,
        "a notice its hook didn't confirm is told again"
    );
    assert!(sessions.told(&store, &s.key, "1").unwrap());
    assert!(!sessions.told(&store, &s.key, "1").unwrap(), "already told");
    assert_eq!(sessions.hook(&store, &s.key, Tier::Turn).unwrap().map(|h| h.unread), Some(0), "told once");
    sessions.post(&store, &s.key, letter("2")).unwrap();
    assert_eq!(
        sessions.hook(&store, &s.key, Tier::Turn).unwrap().map(|h| h.unread),
        Some(2),
        "a new letter: told again"
    );
    assert!(sessions.told(&store, &s.key, "2").unwrap());
    assert!(!sessions.told(&store, &s.key, "1").unwrap());
    assert_eq!(
        sessions.hook(&store, &s.key, Tier::Turn).unwrap().map(|h| h.unread),
        Some(0),
        "a late older confirmation undoes nothing"
    );
    assert_eq!(sessions.by_key(&store, &s.key).unwrap().tier, Tier::Turn);
}

#[test]
fn a_tier_only_rises_and_wakes_are_capped() {
    let store = store();
    let sessions = Sessions::default();
    let (s, _) = sessions
        .register(&store, &caller(AgentKind::ClaudeCode, Some("c1"), "/tmp"), None, None, || Ok("api-claude".into()))
        .unwrap();
    assert!(sessions.hook(&store, &s.key, Tier::Push).unwrap().unwrap().raised);
    assert!(!sessions.hook(&store, &s.key, Tier::Turn).unwrap().unwrap().raised);
    assert_eq!(sessions.by_key(&store, &s.key).unwrap().tier, Tier::Push, "never lowered");
    assert!((0..WAKES_PER_HOUR).all(|_| sessions.may_wake(&s.key, "1") == Wake::Allowed), "asking spends nothing");
    (0..WAKES_PER_HOUR).for_each(|_| sessions.woke(&s.key));
    assert_eq!(sessions.may_wake(&s.key, "1"), Wake::Limited, "an hour's wakes are used up");
    assert_eq!(sessions.may_wake(&s.key, "1"), Wake::StillLimited, "said once per letter");
    assert_eq!(sessions.may_wake(&s.key, "2"), Wake::Limited);
    assert_eq!(sessions.may_wake("claude-code:other", "1"), Wake::Allowed, "per session");
}

#[test]
fn the_apps_list_recent_named_sessions_and_hear_of_changes() {
    let (store, sessions, mut book) = (store(), Sessions::default(), book());
    let mut reg = |c: &Caller, title: Option<&str>| {
        sessions.register(&store, c, title.map(str::to_owned), None, || book("api-codex")).unwrap().1
    };
    let codex = caller(AgentKind::Codex, Some("t"), "/work/api");
    let (new, same, changed) = (
        Registered { new: true, offer: true },
        Registered { new: false, offer: false },
        Registered { new: false, offer: true },
    );
    assert_eq!(reg(&codex, Some("Fix login")), new);
    assert_eq!(reg(&codex, Some("Fix login")), same);
    assert_eq!(reg(&codex, None), same, "no title said keeps the one it had");
    assert_eq!(reg(&codex, Some("Ship it")), changed);
    assert_eq!(
        reg(&caller(AgentKind::Other, None, "/"), None),
        Registered { new: true, offer: false },
        "temporary: not offered"
    );
    let listed = sessions.listed(&store, |_| false);
    assert_eq!(
        listed.iter().map(|l| l.session.title.as_deref()).collect::<Vec<_>>(),
        [Some("Ship it")],
        "temporary ones aren't"
    );
    assert_eq!(registry(AgentKind::Codex), Some("codex-acp"));
}

#[test]
fn running_sessions_come_first_old_ones_fold_and_a_rule_keeps_one_listed() {
    let (store, sessions, mut book) = (store(), Sessions::default(), book());
    let now = now_ms();
    let mut at = |session: &str, ago: i64| {
        let c = caller(AgentKind::Codex, Some(session), &format!("/work/{session}"));
        let key = register(&sessions, &store, &c, &mut book).0.key;
        let mut all = load(&store);
        all.get_mut(&key).unwrap().seen = now - ago;
        save(&store, &all).unwrap();
        key
    };
    let hour = 60 * 60 * 1000;
    at("web", hour);
    let billing = at("billing", 3 * hour);
    at("infra", 3 * DAY_MS);
    let ops = at("ops", 3 * DAY_MS);
    let rule = at("old", LISTED_FOR_MS + hour);
    sessions.set_live(&billing, Some(Live::Working));
    sessions.set_live(&ops, Some(Live::Offline));
    let order = |kept: &dyn Fn(&Session) -> bool| {
        let listed = sessions.listed(&store, kept);
        listed.iter().map(|l| (l.session.address.clone(), l.older)).collect::<Vec<_>>()
    };
    let shown = |a: &str, older| (a.to_owned(), older);
    assert_eq!(
        order(&|_| false),
        [
            shown("billing-codex", false),
            shown("web-codex", false),
            shown("infra-codex", true),
            shown("ops-codex", true)
        ],
        "running first, then the newest; a week unseen leaves the list"
    );
    assert_eq!(order(&|s| s.key == rule).last(), Some(&shown("old-codex", true)), "a rule that opens it keeps it");
}

#[test]
fn the_apps_hear_only_of_a_change_in_what_a_session_does() {
    let sessions = Sessions::default();
    assert!(sessions.set_live("a", Some(Live::Idle)));
    assert!(!sessions.set_live("a", Some(Live::Idle)));
    assert!(sessions.set_live("a", Some(Live::Working)));
    assert!(sessions.set_live("a", None), "nothing can tell any more");
    assert!(!sessions.set_live("a", None));
    sessions.set_live("gone", Some(Live::Online));
    let later = now_ms() + 1;
    assert!(
        sessions.renew_live(HashMap::from([("a".to_owned(), Live::Idle)]), later),
        "one no longer listed is forgotten"
    );
    assert!(!sessions.renew_live(HashMap::from([("a".to_owned(), Live::Idle)]), later));
    // A session's own call during a look wins over what the look found.
    let started = now_ms() - 1;
    sessions.set_live("b", Some(Live::Online));
    sessions.renew_live(HashMap::from([("b".to_owned(), Live::Offline)]), started);
    assert_eq!(sessions.live.locked().get("b").map(|e| e.0), Some(Live::Online));
}

#[cfg(unix)]
#[test]
fn a_session_whose_process_moved_on_to_another_is_offline() {
    // This test's own process: it runs.
    let me = std::process::id();
    let process = crate::dev_servers::process_start(me).map(|started| (me, started));
    let session = |key: &str, seen: i64| Session {
        key: key.into(),
        kind: AgentKind::ClaudeCode,
        address: key.into(),
        cwd: "/".into(),
        title: None,
        busy: None,
        temporary: false,
        named: None,
        seen,
        tier: Tier::Pull,
        announced: None,
        running: process.clone(),
    };
    let (old, new) = (session("old", 1), session("new", 2));
    let all = [old.clone(), new.clone()];
    assert_eq!(Live::read(&old, &all), Some(Live::Offline), "after /clear its process serves the newer one");
    let codex = |s: Session| Session { kind: AgentKind::Codex, ..s };
    let all = [codex(old.clone()), codex(new)];
    assert_eq!(Live::read(&all[0], &all), Some(Live::Online), "Codex threads share a process");
}
