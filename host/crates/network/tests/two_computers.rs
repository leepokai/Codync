//! Computers on this machine, connected directly (no relay, no internet).

use codync_network::{Address, Agent, Connector, Letter, Message, Node, Reach, Role, Sent};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc::Receiver;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("codync-network-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn agent(id: &str) -> Agent {
    Agent { id: id.into(), name: id.into(), about: String::new() }
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
