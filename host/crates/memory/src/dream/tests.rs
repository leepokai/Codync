use super::*;
use crate::files::{Author, Cause, Change};

mod notes;

fn db() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    crate::schema::migrate(&conn).unwrap();
    conn
}

fn user(id: &str, text: &str) -> Message {
    Message { id: id.into(), from_user: true, text: vec![text.into()], date: "2026-10-17".into() }
}

fn bot_reply(id: &str, text: &str) -> Message {
    Message { id: id.into(), from_user: false, text: vec![text.into()], date: "2026-10-17".into() }
}

fn add(text: &str, quote: &str, message: &str) -> Add {
    Add { text: text.into(), shared: false, quote: quote.into(), message: message.into() }
}

fn remove(line: &str, quote: &str, message: &str) -> Remove {
    Remove { line: line.into(), shared: false, quote: quote.into(), message: message.into() }
}

/// Writes bot `a`'s MEMORY.md as `author` wrote it.
fn seed(conn: &Connection, author: Author, content: &str) {
    write(conn, Some("a"), author, MAIN, content);
}

/// Writes the file `name` of `bot`'s layer (`None`: the shared one) as `author` wrote it.
fn write(conn: &Connection, bot: Option<&str>, author: Author, name: &str, content: &str) {
    let change = Change {
        bot,
        name,
        content: Some(content),
        author,
        author_bot: None,
        cause: Cause::Edit,
        message: None,
        mtime: 1,
        size: 1,
    };
    files::record(conn, &change, 1, 1).unwrap();
}

/// `facts` as MEMORY.md lines.
fn lines(facts: impl IntoIterator<Item = String>) -> String {
    facts.into_iter().map(|fact| format!("- {fact}\n")).collect::<Vec<_>>().concat()
}

/// What a run would write to bot `a`'s MEMORY.md.
fn main_after(edits: &Edits) -> &str {
    edits.files.iter().find(|e| e.name == MAIN).map_or("", |e| e.after.as_str())
}

#[test]
fn keeps_only_changes_backed_by_the_users_own_words() {
    let conn = db();
    let snapshot = snapshot(&conn, "a", false, "2026-10-17").unwrap();
    let messages =
        [user("u1", "I moved to Tainan last month"), bot_reply("b1", "Noted that you love durian"), user("u2", "yes")];
    let plan = Plan {
        add: vec![
            add("Lives in Tainan", "I moved to Tainan", "m1"),
            add("Loves durian", "you love durian", "m2"),
            add("Agreed to something", "yes", "m3"),
            add("Made up", "never said this", "m1"),
            add("Password is hunter22", "I moved to Tainan", "m1"),
        ],
        ..Plan::default()
    };
    let (kept, rejected) = audit(&snapshot, &messages, plan);
    assert_eq!(kept.add.iter().map(|a| a.text.as_str()).collect::<Vec<_>>(), ["Lives in Tainan"]);
    assert_eq!(rejected.len(), 4);
    let (edits, _) = edits(&conn, &snapshot, &kept, &messages).unwrap();
    assert_eq!(main_after(&edits), "- Lives in Tainan\n");
    assert_eq!(
        edits.added,
        [Line { bot: Some("a".into()), text: "Lives in Tainan".into(), message: Some("u1".into()), rewrites: None }]
    );
}

#[test]
fn lines_written_on_the_computer_are_the_users() {
    let conn = db();
    let filler = lines((0..7).map(|i| format!("Filler {i}")));
    seed(&conn, Author::Computer, &format!("- Vegetarian since 2020\n{filler}"));
    let snapshot = snapshot(&conn, "a", false, "2026-10-17").unwrap();
    let messages = [user("u1", "I eat meat again these days")];
    let plan = Plan {
        remove: vec![remove("Vegetarian since 2020", "I eat meat again", "m1")],
        review: vec![Review { line: "Vegetarian since 2020".into(), shared: false, text: "Was vegetarian".into() }],
        ..Plan::default()
    };
    let (kept, rejected) = audit(&snapshot, &messages, plan);
    assert!(kept.is_empty() && rejected.iter().all(|r| r.contains("the user wrote it")), "{rejected:?}");
    assert!(
        prompt(&snapshot, &messages).1.contains("This bot's (\"shared\": false), written by the user:\n- Vegetarian")
    );
}

#[test]
fn a_run_writes_only_what_it_changed() {
    let conn = db();
    seed(&conn, Author::Bot, "- Has two cats\n- Lives in Taipei");
    let snapshot = snapshot(&conn, "a", false, "2026-10-17").unwrap();
    assert_eq!(edits(&conn, &snapshot, &Plan::default(), &[]).unwrap().0, Edits::default());
}

#[test]
fn a_forgotten_line_stays_out_of_notes_too() {
    let conn = db();
    store::bury(&conn, Some("a"), ["Sister Amy is pregnant"], ["u2"]).unwrap();
    let snapshot = snapshot(&conn, "a", false, "2026-10-17").unwrap();
    let messages = [user("u1", "my sister Amy is pregnant again"), user("u2", "Amy's baby is due in May")];
    let append = |text: &str, quote: &str, message: &str| Append {
        file: "people.md".into(),
        shared: false,
        text: text.into(),
        quote: quote.into(),
        message: message.into(),
        description: "The user's family".into(),
    };
    let plan = Plan {
        append: vec![
            append("Sister Amy is pregnant", "my sister Amy is pregnant", "m1"),
            append("Amy's baby is due in May 2027", "Amy's baby is due in May", "m2"),
        ],
        ..Plan::default()
    };
    let (kept, _) = audit(&snapshot, &messages, plan);
    assert_eq!(kept.append.len(), 2);
    let (edits, rejected) = edits(&conn, &snapshot, &kept, &messages).unwrap();
    assert!(edits.files.is_empty(), "{edits:?}");
    assert_eq!(rejected.iter().filter(|r| r.contains("forgotten")).count(), 2, "{rejected:?}");
}

#[test]
fn never_removes_the_users_words_or_more_than_a_quarter() {
    let conn = db();
    seed(&conn, Author::User, "- Lives in Taipei\n");
    let facts = lines((0..7).map(|i| format!("Fact number {i}")));
    let mut all = "- Lives in Taipei\n".to_owned();
    all.push_str(&facts);
    seed(&conn, Author::Bot, &all);
    let snapshot = snapshot(&conn, "a", false, "2026-10-17").unwrap();
    let messages = [user("u1", "forget all of those old facts please")];
    let quote = "forget all of those old facts";

    let (kept, rejected) =
        audit(&snapshot, &messages, Plan { remove: vec![remove("Lives in Taipei", quote, "m1")], ..Plan::default() });
    assert!(kept.is_empty() && rejected[0].contains("the user wrote it"), "{rejected:?}");

    let three = (0..3).map(|i| remove(&format!("Fact number {i}"), quote, "m1")).collect();
    let (kept, rejected) = audit(&snapshot, &messages, Plan { remove: three, ..Plan::default() });
    assert!(kept.is_empty() && rejected.last().unwrap().contains("more than a quarter"));

    let two = (0..2).map(|i| remove(&format!("Fact number {i}"), quote, "m1")).collect();
    let (kept, _) = audit(&snapshot, &messages, Plan { remove: two, ..Plan::default() });
    let (edits, _) = edits(&conn, &snapshot, &kept, &messages).unwrap();
    assert!(!main_after(&edits).contains("Fact number 0") && main_after(&edits).contains("Fact number 2"));
    assert_eq!(edits.removed.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["Fact number 0", "Fact number 1"]);
}

#[test]
fn a_replacement_takes_the_old_line_out_and_forgotten_lines_stay_out() {
    let conn = db();
    seed(&conn, Author::Bot, "- Lives in Taipei\n- Likes tea\n- Has two cats\n- Works late\n");
    let snapshot = snapshot(&conn, "a", false, "2026-10-17").unwrap();
    let messages =
        [user("u1", "We just moved to Tainan for good"), user("u2", "I quit coffee and my sister Mei visits monthly")];
    store::bury(&conn, Some("a"), ["Quit coffee"], ["u9"]).unwrap();
    let plan = Plan {
        add: vec![
            add("Lives in Tainan", "just moved to Tainan", "m1"),
            add("Quit coffee", "I quit coffee and my sister Mei", "m2"),
            add("Likes tea", "I quit coffee and my sister", "m2"),
        ],
        remove: vec![remove("Lives in Taipei", "We just moved to Tainan", "m1")],
        ..Plan::default()
    };
    let (kept, _) = audit(&snapshot, &messages, plan);
    let (edits, rejected) = edits(&conn, &snapshot, &kept, &messages).unwrap();
    assert_eq!(main_after(&edits), "- Likes tea\n- Has two cats\n- Works late\n- Lives in Tainan\n");
    assert!(rejected.iter().any(|r| r.contains("Quit coffee") && r.contains("forgotten")), "{rejected:?}");
    assert!(rejected.iter().any(|r| r.contains("Likes tea") && r.contains("already")), "{rejected:?}");
}

#[test]
fn a_line_copied_from_the_prompt_names_its_fact() {
    let conn = db();
    seed(&conn, Author::Bot, "- Lives in Hsinchu.\n");
    let snapshot = snapshot(&conn, "a", false, "2026-10-17").unwrap();
    let messages = [user("u1", "please forget where I live")];
    let (_, material) = prompt(&snapshot, &messages);
    let shown = material.lines().find(|l| l.contains("Hsinchu")).expect("the line is shown");
    let plan = Plan { remove: vec![remove(shown, "forget where I live", "m1")], ..Plan::default() };
    let (kept, rejected) = audit(&snapshot, &messages, plan);
    assert!(rejected.is_empty(), "{rejected:?}");
    let (edits, _) = edits(&conn, &snapshot, &kept, &messages).unwrap();
    assert_eq!(main_after(&edits), "");
}

#[test]
fn reviews_rewrite_dated_lines_in_the_past_tense_once() {
    let conn = db();
    let filler = lines((0..6).map(|i| format!("Filler {i}")));
    seed(&conn, Author::Dream, &format!("- Going to Singapore in July 2026\n- Likes tea\n{filler}"));
    let snapshot = snapshot(&conn, "a", false, "2026-10-17").unwrap();
    let review = |line: &str, text: &str| Review { line: line.into(), shared: false, text: text.into() };
    let plan = Plan {
        review: vec![
            review("Going to Singapore in July 2026", "Went to Singapore in July 2026"),
            review("Likes tea", "Liked tea"),
        ],
        ..Plan::default()
    };
    let (kept, rejected) = audit(&snapshot, &[], plan);
    assert_eq!(kept.review.len(), 1);
    assert!(rejected[0].contains("no date"));
    let (first, _) = edits(&conn, &snapshot, &kept, &[]).unwrap();
    assert!(main_after(&first).ends_with("- Went to Singapore in July 2026\n"), "{}", main_after(&first));
    assert!(!main_after(&first).contains("Going to"));
    let rewrite = &first.added[0];
    assert_eq!(rewrite.text, "Went to Singapore in July 2026");
    let source = snapshot.fact(false, "Going to Singapore in July 2026").cloned();
    assert_eq!((&rewrite.message, &rewrite.rewrites), (&source.as_ref().unwrap().message, &source));

    // The next day's review leaves its own rewrite alone, once the caller marked it written.
    mark_reviewed(&conn, Some("a"), &rewrite.text).unwrap();
    seed(&conn, Author::Dream, main_after(&first));
    let snapshot = super::snapshot(&conn, "a", false, "2026-10-18").unwrap();
    let again = Plan { review: vec![review("Went to Singapore in July 2026", "Went to Singapore")], ..Plan::default() };
    let (kept, _) = audit(&snapshot, &[], again);
    assert_eq!(edits(&conn, &snapshot, &kept, &[]).unwrap().0.files, [] as [Edit; 0]);

    // A rewrite the user had forgotten isn't written back: the dated line stays as it is.
    let conn = db();
    seed(&conn, Author::Dream, &format!("- Going to Singapore in July 2026\n{filler}"));
    store::bury(&conn, Some("a"), ["Went to Singapore in July 2026"], []).unwrap();
    let snapshot = super::snapshot(&conn, "a", false, "2026-10-17").unwrap();
    let past = Plan {
        review: vec![review("Going to Singapore in July 2026", "Went to Singapore in July 2026")],
        ..Plan::default()
    };
    let (kept, _) = audit(&snapshot, &[], past);
    let (unchanged, rejected) = edits(&conn, &snapshot, &kept, &[]).unwrap();
    assert!(unchanged.files.is_empty() && rejected.iter().any(|r| r.contains("forgotten")), "{rejected:?}");
}

#[test]
fn parses_the_outermost_object_and_survives_chatter() {
    let raw = "Sure!\n```json\n{\"add\": [{\"text\": \"Likes tea\", \"quote\": \"I like tea a lot\", \"message\": \"m1\"}]}\n```";
    let plan = parse(raw).unwrap();
    assert_eq!(plan.add[0].text, "Likes tea");
    assert_eq!(plan.remove, [] as [Remove; 0]);
    assert_eq!(parse("no json here"), None);
}

#[test]
fn the_prompt_numbers_messages_and_marks_the_users_lines() {
    let conn = db();
    seed(&conn, Author::User, "- Lives in Taipei\n");
    let snapshot = snapshot(&conn, "a", true, "2026-10-17").unwrap();
    let (system, material) = prompt(&snapshot, &[user("u1", "hello there friend"), bot_reply("b1", "hi")]);
    assert!(system.contains("Today is 2026-10-17"));
    assert!(
        material.contains("This bot's (\"shared\": false), written by the user:\n- Lives in Taipei\n"),
        "{material}"
    );
    assert!(material.contains("[m1] User, 2026-10-17:\nhello there friend"));
    assert!(material.contains("[m2] Bot"));
    assert!(material.contains("Notes now:\n(none)"));
    assert!(!system.contains("moveOut"), "only offered when memory is nearly full");
}
