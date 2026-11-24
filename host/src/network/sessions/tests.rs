use super::*;

fn store() -> Store {
    let dir = std::env::temp_dir().join(format!("codync-sessions-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    Store::open(&dir.join("t.db")).unwrap()
}

fn caller(kind: AgentKind, session: Option<&str>, cwd: &str) -> Caller {
    Caller { kind, session: session.map(str::to_owned), process: "7-a".into(), cwd: cwd.into(), hint: None }
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
) -> (Session, bool) {
    let (info, base) = caller.look();
    sessions.register(store, caller, info.title, || book(&base)).unwrap()
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
    register(&sessions, &store, &caller(AgentKind::Codex, Some("now"), "/work/api"), &mut book);
    let left: Vec<String> = sessions.all(&store).into_iter().map(|s| s.key).collect();
    assert!(!left.contains(&quiet.key) && !left.contains(&temporary.key), "{left:?}");
    assert!(left.contains(&waiting.key), "its mail keeps it: {left:?}");
    assert_eq!(sessions.unread(&store, &temporary.key), 0, "a forgotten session's mail goes with it");
}
