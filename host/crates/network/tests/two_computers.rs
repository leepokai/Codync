//! Computers on this machine, connected directly (no relay, no internet).

use codync_network::{Address, Agent, AgentKind, Connector, Letter, Message, Node, Reach, Role, Sent};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("codync-network-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn agent(id: &str) -> Agent {
    Agent { id: id.into(), name: id.into(), about: String::new(), kind: AgentKind::Codync }
}

fn offer(node: &Node, id: &str) {
    node.set_agents([(agent(id), Reach::App)]);
}

async fn next(inbox: &mut Receiver<Message>) -> Message {
    tokio::time::timeout(Duration::from_secs(5), inbox.recv()).await.expect("a message within 5 s").expect("running")
}

fn to(address: &str) -> Address {
    address.parse().expect("an address")
}

#[tokio::test]
async fn invite_join_list_and_message() {
    let (desk_dir, laptop_dir) = (temp_dir("desk"), temp_dir("laptop"));
    let (desk, mut desk_inbox) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    let code = desk.create("desk").unwrap();
    assert_eq!(code.len(), 71, "64 hex characters in groups of 8");
    offer(&desk, "reviewer");
    let (laptop, _laptop_inbox) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    offer(&laptop, "coder");

    let invite = desk.invite().unwrap();
    laptop.join(&invite, "laptop").await.unwrap();
    assert!(laptop.join(&invite, "again").await.is_err(), "already in a network");
    assert_eq!((desk.role(), laptop.role()), (Some(Role::Owner), Some(Role::Member)));
    assert!(laptop.invite().unwrap_err().to_string().contains("only the owner and admins"));
    // The invite was single use.
    let (third, _) = Node::start(&temp_dir("third"), &Connector::Private).await.unwrap();
    assert!(third.join(&invite, "third").await.unwrap_err().to_string().contains("expired or was already used"));

    let peers = laptop.peers().await.unwrap();
    let names: Vec<_> = peers.iter().map(|p| (p.name.as_str(), p.online)).collect();
    assert_eq!(names, [("laptop", true), ("desk", true)]);
    assert_eq!(peers[1].agents, [agent("reviewer")]);
    // A bare name finds the one computer that has it.
    assert_eq!(laptop.resolve("reviewer").unwrap(), to("reviewer@desk"));
    assert!(laptop.resolve("nobody").unwrap_err().to_string().contains("no agent called nobody"));

    let letter = Letter::new("coder", to("reviewer@desk"), "review src/auth.rs");
    assert_eq!(laptop.send(letter.clone()).await.unwrap(), Sent::Delivered);
    let got = next(&mut desk_inbox).await;
    assert_eq!(
        (got.from.to_string().as_str(), got.to.as_str(), got.body.as_str(), got.wake),
        ("coder@laptop", "reviewer", "review src/auth.rs", true)
    );
    // A retry of the same letter is taken once; the same words in a new letter are a repeat.
    assert_eq!(laptop.send(letter).await.unwrap(), Sent::Delivered);
    let repeat = laptop.send(Letter::new("coder", to("reviewer@desk"), "review src/auth.rs")).await.unwrap_err();
    assert!(repeat.to_string().contains("isn't delivered twice"), "{repeat}");

    // A notice comes with wake off, and meta passes through untouched.
    let notice = Letter::new("coder", to("reviewer@desk"), "FYI: green").notice().meta(serde_json::json!({"ci": 1}));
    laptop.send(notice).await.unwrap();
    let got = next(&mut desk_inbox).await;
    assert!(!got.id.is_empty() && !got.wake && got.meta == Some(serde_json::json!({"ci": 1})));

    // Quiet by design: a pair of agents gets a small burst, then the receiver says slow down.
    for n in 0..3 {
        laptop.send(Letter::new("coder", to("reviewer@desk"), format!("and file {n}"))).await.unwrap();
    }
    let chatty = laptop.send(Letter::new("coder", to("reviewer@desk"), "one more")).await.unwrap_err().to_string();
    assert!(chatty.contains("slow down"), "{chatty}");
    let deep = laptop.send(Letter::new("coder", to("reviewer@desk"), "deep chain").hops(9)).await.unwrap_err();
    assert!(deep.to_string().contains("handoffs"));
    assert!(
        laptop.send(Letter::new("coder", to("nobody@desk"), "hi")).await.unwrap_err().to_string().contains("no agent")
    );
    let stranger = laptop.send(Letter::new("someone", to("reviewer@desk"), "hi")).await.unwrap_err();
    assert!(stranger.to_string().contains("isn't one of this computer's agents"));

    // Only the owner removes; a removed computer is told so and can't get back in with its old key.
    assert!(laptop.remove("desk", false).is_err());
    desk.remove("laptop", false).unwrap();
    let refused = laptop.send(Letter::new("coder", to("reviewer@desk"), "still here?")).await;
    assert!(refused.unwrap_err().to_string().contains("was removed"));

    // The roster survives a restart.
    desk.shutdown().await;
    let (desk, _) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    assert_eq!(desk.name().as_deref(), Some("desk"));
    for dir in [desk_dir, laptop_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn admins_invite_names_get_numbers_and_the_recovery_code_moves_the_owner() {
    let dirs = [temp_dir("o-desk"), temp_dir("o-server"), temp_dir("o-laptop")];
    let (desk, _) = Node::start(&dirs[0], &Connector::Private).await.unwrap();
    let code = desk.create("desk").unwrap();
    let (server, _) = Node::start(&dirs[1], &Connector::Private).await.unwrap();
    server.join(&desk.invite().unwrap(), "server").await.unwrap();
    desk.promote("server").unwrap();
    // The promotion reaches the server on the next roster swap.
    desk.peers().await.unwrap();
    assert_eq!(server.role(), Some(Role::Admin));
    let (laptop, _) = Node::start(&dirs[2], &Connector::Private).await.unwrap();
    laptop.join(&server.invite().unwrap(), "desk").await.unwrap();
    assert_eq!(laptop.name().as_deref(), Some("desk-2"), "a taken name gets a number");

    // The owner is lost: the recovery code makes the server the owner, and it removes the desk.
    desk.shutdown().await;
    server.recover(&code).unwrap();
    server.remove("desk", false).unwrap();
    assert_eq!(server.role(), Some(Role::Owner));
    laptop.peers().await.unwrap();
    let names: Vec<String> = laptop.directory().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["desk-2", "server"]);
    assert!(server.recover("00000000-00000000-00000000-00000000-00000000-00000000-00000000-00000000").is_err());
    for dir in dirs {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn letters_for_an_away_computer_wait() {
    let (desk_dir, laptop_dir) = (temp_dir("desk2"), temp_dir("laptop2"));
    let (desk, _) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    desk.create("desk").unwrap();
    offer(&desk, "reviewer");
    let (laptop, _) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    offer(&laptop, "coder");
    laptop.join(&desk.invite().unwrap(), "laptop").await.unwrap();
    laptop.shutdown().await;

    assert_eq!(desk.send(Letter::new("reviewer", to("coder@laptop"), "when you're back")).await.unwrap(), Sent::Queued);

    // Back on: its hello to the desk on start flushes the letter.
    let (laptop, mut inbox) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    offer(&laptop, "coder");
    assert_eq!(next(&mut inbox).await.body, "when you're back");
    for dir in [desk_dir, laptop_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn the_same_answer_to_two_letters_is_no_repeat() {
    let (desk_dir, laptop_dir) = (temp_dir("desk3"), temp_dir("laptop3"));
    let (desk, mut inbox) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    desk.create("desk").unwrap();
    offer(&desk, "lead");
    let (laptop, _) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    offer(&laptop, "reviewer");
    laptop.join(&desk.invite().unwrap(), "laptop").await.unwrap();
    for request in ["a", "b"] {
        laptop.send(Letter::new("reviewer", to("lead@desk"), "Done.").reply_to(request)).await.unwrap();
        assert_eq!(next(&mut inbox).await.reply_to.as_deref(), Some(request));
    }
    let again = laptop.send(Letter::new("reviewer", to("lead@desk"), "Done.").reply_to("b")).await.unwrap_err();
    assert!(again.to_string().contains("isn't delivered twice"), "{again}");
    for dir in [desk_dir, laptop_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn without_a_network_agents_on_one_computer_still_reach_each_other() {
    let dir = temp_dir("alone");
    let (node, mut inbox) = Node::start(&dir, &Connector::Private).await.unwrap();
    node.set_agents([(agent("a"), Reach::App), (agent("b"), Reach::App)]);
    let to = node.resolve("b").unwrap();
    assert_eq!(node.send(Letter::new("a", to, "hi")).await.unwrap(), Sent::Delivered);
    assert_eq!(next(&mut inbox).await.from.to_string(), "a@local");
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_computer_still_starting_gets_its_letters_once_it_offers_its_agents() {
    let (desk_dir, laptop_dir) = (temp_dir("desk4"), temp_dir("laptop4"));
    let (desk, _) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    desk.create("desk").unwrap();
    offer(&desk, "lead");
    let (laptop, _) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    offer(&laptop, "coder");
    laptop.join(&desk.invite().unwrap(), "laptop").await.unwrap();
    laptop.shutdown().await;
    assert_eq!(desk.send(Letter::new("lead", to("coder@laptop"), "while you were off")).await.unwrap(), Sent::Queued);

    // Back on, but its app hasn't offered the agents yet: the desk's letter waits instead of going.
    let (laptop, mut inbox) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(inbox.try_recv().is_err());
    offer(&laptop, "coder");
    assert_eq!(next(&mut inbox).await.body, "while you were off");
    for dir in [desk_dir, laptop_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn a_reply_reaches_an_agent_renamed_since_it_asked_and_carries_the_chain_on() {
    let (desk_dir, laptop_dir) = (temp_dir("desk5"), temp_dir("laptop5"));
    let (desk, mut desk_inbox) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    desk.create("desk").unwrap();
    offer(&desk, "lead");
    let (laptop, mut laptop_inbox) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    offer(&laptop, "reviewer");
    laptop.join(&desk.invite().unwrap(), "laptop").await.unwrap();

    let ask = Letter::new("lead", to("reviewer@laptop"), "review this").hops(3);
    let asked = ask.id.clone();
    desk.send(ask).await.unwrap();
    assert_eq!(next(&mut laptop_inbox).await.hops, 3);
    // The lead is renamed while the review runs: the desk no longer offers "lead".
    offer(&desk, "lead-2");
    laptop.send(Letter::new("reviewer", to("lead@desk"), "looks good").reply_to(asked.clone())).await.unwrap();
    let reply = next(&mut desk_inbox).await;
    assert_eq!((reply.reply_to.as_deref(), reply.hops), (Some(asked.as_str()), 4));
    // A new letter (no reply_to) for an agent that's gone is still refused, and so is an answer to
    // something the desk never asked.
    let gone = laptop.send(Letter::new("reviewer", to("lead@desk"), "and another thing")).await.unwrap_err();
    assert!(gone.to_string().contains("no agent called lead"), "{gone}");
    let made_up = Letter::new("reviewer", to("lead@desk"), "psst").reply_to("not-a-letter-of-yours");
    let made_up = laptop.send(made_up).await.unwrap_err();
    assert!(made_up.to_string().contains("no agent called lead"), "{made_up}");
    for dir in [desk_dir, laptop_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn changes_tick_when_who_is_in_or_around_changes() {
    let (desk_dir, laptop_dir) = (temp_dir("desk6"), temp_dir("laptop6"));
    let (desk, _) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    let mut changes = desk.changes();
    desk.create("desk").unwrap();
    assert!(changes.has_changed().unwrap(), "creating the network");
    changes.mark_unchanged();

    let (laptop, _) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    offer(&laptop, "coder");
    laptop.join(&desk.invite().unwrap(), "laptop").await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), changes.changed()).await.expect("a join within 5 s").unwrap();
    desk.peers().await.unwrap();
    changes.mark_unchanged();

    // Asking again with nothing new is quiet.
    desk.peers().await.unwrap();
    assert!(!changes.has_changed().unwrap());
    desk.promote("laptop").unwrap();
    assert!(changes.has_changed().unwrap(), "a promotion");
    changes.mark_unchanged();
    offer(&laptop, "reviewer");
    desk.peers().await.unwrap();
    assert!(changes.has_changed().unwrap(), "what the laptop offers");
    changes.mark_unchanged();
    laptop.shutdown().await;
    desk.peers().await.unwrap();
    assert!(changes.has_changed().unwrap(), "the laptop went away");
    assert!(!desk.directory()[1].online);
    for dir in [desk_dir, laptop_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn roles_decide_who_invites_removes_and_promotes() {
    use Role::{Admin, Member, Owner};
    assert!(Owner.invites() && Admin.invites() && !Member.invites());
    assert!(Owner.removes(Admin) && Owner.removes(Member) && Admin.removes(Member));
    assert!(!Admin.removes(Admin) && !Admin.removes(Owner) && !Member.removes(Member));
    assert!(Owner.promotes(Member) && !Owner.promotes(Admin) && !Admin.promotes(Member));
}

#[tokio::test]
async fn the_receiving_computers_rules_decide_but_answers_to_the_asker_pass() {
    use codync_network::{Access, Rule};
    let (desk_dir, laptop_dir) = (temp_dir("desk7"), temp_dir("laptop7"));
    let (desk, mut desk_inbox) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    desk.create("desk").unwrap();
    desk.set_agents([(agent("reviewer"), Reach::App), (agent("coder"), Reach::App)]);
    let (laptop, mut laptop_inbox) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    laptop.set_agents([(agent("lead"), Reach::App), (agent("intern"), Reach::App)]);
    laptop.join(&desk.invite().unwrap(), "laptop").await.unwrap();

    // The desk asks the laptop's lead something before the laptop locks the desk out.
    let ask = Letter::new("reviewer", to("lead@laptop"), "which branch?");
    let asked = ask.id.clone();
    desk.send(ask).await.unwrap();
    next(&mut laptop_inbox).await;
    desk.set_rules(vec![
        Rule::new(Access::Allow, "lead@laptop", "reviewer").unwrap(),
        Rule::new(Access::Deny, "*@laptop", "*").unwrap(),
    ])
    .unwrap();
    laptop.send(Letter::new("lead", to("reviewer@desk"), "review this")).await.unwrap();
    assert_eq!(next(&mut desk_inbox).await.body, "review this", "allowed by the first rule");
    let refused = laptop.send(Letter::new("intern", to("coder@desk"), "hi")).await.unwrap_err();
    assert_eq!(refused.to_string(), "desk refused it: coder doesn't take messages from intern@laptop");
    // An answer to the desk's own question still gets in, whoever sends it, but only to the
    // agent that asked, and only until the one that says how it ended (`meta`).
    let elsewhere = Letter::new("intern", to("coder@desk"), "main").reply_to(asked.clone());
    assert!(laptop.send(elsewhere).await.is_err(), "coder didn't ask");
    let answer = Letter::new("intern", to("reviewer@desk"), "main").reply_to(asked.clone());
    assert_eq!(laptop.send(answer).await.unwrap(), Sent::Delivered);
    assert_eq!(next(&mut desk_inbox).await.body, "main");
    let last = Letter::new("intern", to("reviewer@desk"), "done").reply_to(asked.clone()).meta(serde_json::json!({}));
    assert_eq!(laptop.send(last).await.unwrap(), Sent::Delivered);
    assert_eq!(next(&mut desk_inbox).await.body, "done");
    assert!(laptop.send(Letter::new("intern", to("reviewer@desk"), "more").reply_to(asked)).await.is_err());
    // A made-up answer doesn't.
    let fake = Letter::new("intern", to("coder@desk"), "sneaky").reply_to("not-a-letter-of-yours");
    assert!(laptop.send(fake).await.is_err());
    for dir in [desk_dir, laptop_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn an_unlisted_sender_asks_and_hears_back_but_isnt_offered() {
    let (desk_dir, laptop_dir) = (temp_dir("desk8"), temp_dir("laptop8"));
    let (desk, mut desk_inbox) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    desk.create("desk").unwrap();
    offer(&desk, "reviewer");
    desk.set_senders(["assistant".to_owned()]);
    let (laptop, mut laptop_inbox) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    offer(&laptop, "coder");
    laptop.join(&desk.invite().unwrap(), "laptop").await.unwrap();

    let ask = Letter::new("assistant", to("coder@laptop"), "status?");
    let asked = ask.id.clone();
    desk.send(ask).await.unwrap();
    let got = next(&mut laptop_inbox).await;
    assert_eq!(got.from, to("assistant@desk"));
    laptop.send(Letter::new("coder", got.from, "all green").reply_to(asked)).await.unwrap();
    assert_eq!(next(&mut desk_inbox).await.body, "all green");
    let offered: Vec<String> = laptop.peers().await.unwrap()[1].agents.iter().map(|a| a.id.clone()).collect();
    assert_eq!(offered, ["reviewer"], "the assistant isn't listed");
    assert!(desk.send(Letter::new("nobody", to("coder@laptop"), "hi")).await.is_err());
    for dir in [desk_dir, laptop_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[tokio::test]
async fn an_invite_carries_the_networks_name() {
    let dir = temp_dir("named");
    let (desk, _) = Node::start(&dir, &Connector::Private).await.unwrap();
    desk.create("desk").unwrap();
    assert_eq!(codync_network::read_invite(&desk.invite().unwrap()).unwrap().name, None);
    desk.set_network_name(Some("home".into()));
    let read = codync_network::read_invite(&desk.invite().unwrap()).unwrap();
    assert_eq!((read.name.as_deref(), Some(read.network)), (Some("home"), desk.network_id()));
    desk.shutdown().await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_session_the_network_knocks_on_is_offered_as_its_kind() {
    let (desk_dir, laptop_dir) = (temp_dir("desk9"), temp_dir("laptop9"));
    let (desk, _) = Node::start(&desk_dir, &Connector::Private).await.unwrap();
    desk.create("desk").unwrap();
    offer(&desk, "reviewer");
    let (laptop, _) = Node::start(&laptop_dir, &Connector::Private).await.unwrap();
    let session = Agent { id: "main".into(), name: "main".into(), about: String::new(), kind: AgentKind::Other };
    laptop.set_agents([(session, Reach::ClaudeCode { session: "s1".into() })]);
    laptop.join(&desk.invite().unwrap(), "laptop").await.unwrap();
    let peers = desk.peers().await.unwrap();
    assert_eq!(peers[0].agents[0].kind, AgentKind::Codync);
    assert_eq!(peers[1].agents[0].kind, AgentKind::ClaudeCode, "its reach says what it is");
    // A computer from before kinds says nothing, and a kind this version doesn't know reads as other.
    let old: Agent = serde_json::from_str(r#"{"id": "x", "name": "x"}"#).unwrap();
    let newer: Agent = serde_json::from_str(r#"{"id": "x", "name": "x", "kind": "gemini"}"#).unwrap();
    assert_eq!((old.kind, newer.kind), (AgentKind::Other, AgentKind::Other));
    for dir in [desk_dir, laptop_dir] {
        let _ = std::fs::remove_dir_all(dir);
    }
}
