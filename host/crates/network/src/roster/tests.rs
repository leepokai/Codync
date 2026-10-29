use super::*;

fn key() -> SecretKey {
    SecretKey::generate()
}

fn addr(k: &SecretKey) -> EndpointAddr {
    EndpointAddr::new(k.public())
}

#[test]
fn the_owner_admits_admins_admit_members_cannot() {
    let (desk, server, laptop, phone) = (key(), key(), key(), key());
    let (mut r, _root) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    assert_eq!(r.role(&desk.public()), Some(Role::Owner));
    r.sign(&desk, 2, Change::Admit { id: server.public(), name: "server".into() }).unwrap();
    r.sign(&desk, 3, Change::Promote { id: server.public() }).unwrap();
    r.sign(&server, 4, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    assert_eq!(r.role(&server.public()), Some(Role::Admin));
    assert_eq!(r.role(&laptop.public()), Some(Role::Member));
    // A plain member can neither admit nor remove.
    assert!(r.sign(&laptop, 5, Change::Admit { id: phone.public(), name: "phone".into() }).is_err());
    assert!(r.sign(&laptop, 5, Change::Remove { id: server.public(), cascade: false }).is_err());
    // An admin can't remove another admin or the owner, and can't promote.
    assert!(r.sign(&server, 6, Change::Remove { id: desk.public(), cascade: false }).is_err());
    assert!(r.sign(&server, 6, Change::Promote { id: laptop.public() }).is_err());
    r.sign(&server, 7, Change::Remove { id: laptop.public(), cascade: false }).unwrap();
    assert!(!r.is_member(&laptop.public()) && r.is_removed(&laptop.public()));
}

#[test]
fn copies_converge_whatever_the_order() {
    let (desk, server, laptop, phone) = (key(), key(), key(), key());
    let (mut a, _root) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    a.sign(&desk, 2, Change::Admit { id: server.public(), name: "server".into() }).unwrap();
    let mut c = a.clone();
    a.sign(&desk, 3, Change::Promote { id: server.public() }).unwrap();
    // The owner and the admin each admit a computer at the same time.
    let mut b = a.clone();
    a.sign(&desk, 4, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    b.sign(&server, 4, Change::Admit { id: phone.public(), name: "phone".into() }).unwrap();
    // c knew neither, nor that server is an admin: it hears of b first, a later.
    assert!(c.merge(&b) && c.is_member(&phone.public()));
    assert!(c.merge(&a));
    assert!(a.merge(&b) && b.merge(&a));
    assert_eq!((a.members(), b.members()), (c.members(), c.members()));
    assert_eq!(a.members().len(), 4);
    assert!(!a.merge(&b), "nothing new the second time");
}

#[test]
fn forged_or_foreign_entries_are_dropped() {
    let (desk, mallory) = (key(), key());
    let (mut r, _root) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    let mut forged = r.clone();
    let mut entry = Signed::new(&r.network, &mallory, 2, Change::Admit { id: mallory.public(), name: "m".into() });
    entry.by = desk.public();
    forged.log.insert(entry);
    assert!(!r.merge(&forged));
    let (other, _) = Roster::create(&mallory, "m".into(), addr(&mallory), 1);
    assert!(!r.merge(&other));
    assert_eq!(r.members().len(), 1);
}

#[test]
fn removing_a_stolen_admin_with_cascade_voids_its_admissions() {
    let (desk, server, laptop, evil) = (key(), key(), key(), key());
    let (mut r, _root) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    r.sign(&desk, 2, Change::Admit { id: server.public(), name: "server".into() }).unwrap();
    r.sign(&desk, 3, Change::Promote { id: server.public() }).unwrap();
    r.sign(&server, 4, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    r.sign(&server, 5, Change::Admit { id: evil.public(), name: "evil".into() }).unwrap();
    let mut retired = r.clone();
    retired.sign(&desk, 6, Change::Remove { id: server.public(), cascade: false }).unwrap();
    assert!(retired.is_member(&laptop.public()), "retiring an admin keeps whom it admitted");
    r.sign(&desk, 6, Change::Remove { id: server.public(), cascade: true }).unwrap();
    assert!(!r.is_member(&evil.public()) && !r.is_member(&laptop.public()));
}

#[test]
fn the_recovery_code_moves_the_owner() {
    let (desk, laptop) = (key(), key());
    let (mut r, root) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    r.sign(&desk, 2, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    let code = recovery_code(&root);
    let root = root_from_code(&code).unwrap();
    r.sign(&root, 3, Change::Owner { id: laptop.public(), name: "laptop".into() }).unwrap();
    r.sign(&laptop, 4, Change::Remove { id: desk.public(), cascade: false }).unwrap();
    assert_eq!(r.role(&laptop.public()), Some(Role::Owner));
    assert!(!r.is_member(&desk.public()));
    assert!(root_from_code("nope").is_err());
}

#[test]
fn taken_names_get_a_number() {
    let (desk, other) = (key(), key());
    let (r, _) = Roster::create(&desk, "laptop".into(), addr(&desk), 1);
    assert_eq!(r.free_name("laptop", &other.public()), "laptop-2");
    assert_eq!(r.free_name("laptop", &desk.public()), "laptop");
    assert_eq!(r.free_name("desk", &other.public()), "desk");
}

#[test]
fn a_computer_moves_itself_only() {
    let (desk, laptop) = (key(), key());
    let (mut r, _) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    r.sign(&desk, 2, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    let moved = addr(&laptop).with_ip_addr("10.0.0.2:1".parse().unwrap());
    r.sign(&laptop, 3, Change::Locate { addr: moved.clone() }).unwrap();
    assert_eq!(r.members()[&laptop.public()].addr, moved);
    let fake = addr(&laptop).with_ip_addr("6.6.6.6:1".parse().unwrap());
    assert!(r.sign(&desk, 4, Change::Locate { addr: fake }).is_err());
}

#[test]
fn entries_out_of_bounds_dont_count_and_a_removed_computer_has_no_location() {
    let (desk, laptop, admin) = (key(), key(), key());
    let (mut r, _) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    r.sign(&desk, 2, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    r.sign(&desk, 3, Change::Admit { id: admin.public(), name: "server".into() }).unwrap();
    r.sign(&desk, 4, Change::Promote { id: admin.public() }).unwrap();
    let size = serde_json::to_vec(&r).unwrap().len();
    // Forty thousand addresses would push every hello past its frame; a name no computer may take.
    let huge = (0..40_000u32).fold(addr(&laptop), |a, n| {
        let [_, b, c, d] = n.to_be_bytes();
        a.with_ip_addr(std::net::SocketAddr::from(([10, b, c, d], 1)))
    });
    let mut junk = r.clone();
    junk.log.insert(Signed::new(&r.network, &laptop, 5, Change::Locate { addr: huge.clone() }));
    let stranger = key();
    let long = Change::Admit { id: stranger.public(), name: "x".repeat(10_000) };
    junk.log.insert(Signed::new(&r.network, &admin, 5, long));
    let dotted = Change::Admit { id: key().public(), name: "a.b".into() };
    junk.log.insert(Signed::new(&r.network, &admin, 5, dotted));
    assert!(!r.merge(&junk), "nothing out of bounds counts");
    assert_eq!(r.members().len(), 3);
    assert!(serde_json::to_vec(&r).unwrap().len() < size + 1024);
    // Its own location is cut to what one may hold, and counts; signing the same again changes
    // nothing, so a computer compares where it is with what it signed, cut the same way.
    r.sign(&laptop, 6, Change::Locate { addr: bounded_addr(huge.clone()) }).unwrap();
    assert_eq!(r.members()[&laptop.public()].addr, bounded_addr(huge.clone()));
    assert_eq!(r.members()[&laptop.public()].addr.addrs.len(), MAX_ADDRS);
    assert!(r.sign(&laptop, 7, Change::Locate { addr: bounded_addr(huge) }).is_err());
    // Once laptop is removed, where it was goes too.
    r.sign(&desk, 8, Change::Remove { id: laptop.public(), cascade: false }).unwrap();
    assert!(!r.log.iter().any(|s| s.by == laptop.public() && matches!(s.change, Change::Locate { .. })));
}

#[test]
fn a_stolen_member_cannot_grow_the_log() {
    let (desk, laptop) = (key(), key());
    let (mut r, _) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    r.sign(&desk, 2, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    let size = serde_json::to_vec(&r).unwrap().len();
    // The thief signs hundreds of locations and admissions, as laptop and as keys nobody let in.
    let mut junk = r.clone();
    for at in 3..400 {
        let stranger = key();
        let moved = addr(&laptop).with_ip_addr(format!("10.0.0.1:{at}").parse().unwrap());
        let admit = Change::Admit { id: stranger.public(), name: "x".into() };
        junk.log.insert(Signed::new(&r.network, &laptop, at, Change::Locate { addr: moved }));
        junk.log.insert(Signed::new(&r.network, &laptop, at, admit));
        junk.log.insert(Signed::new(&r.network, &stranger, at, Change::Locate { addr: addr(&stranger) }));
    }
    assert!(r.merge(&junk), "laptop's newest location counts");
    assert_eq!(r.members().len(), 2);
    assert!(serde_json::to_vec(&r).unwrap().len() < size + 1024, "the log stays as small as it was");
}

#[test]
fn a_stolen_owner_cannot_undo_its_replacement() {
    let (desk, laptop, server) = (key(), key(), key());
    let (mut r, root) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    r.sign(&desk, 2, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    r.sign(&root, 10, Change::Owner { id: laptop.public(), name: "laptop".into() }).unwrap();
    let mut retired = r.clone();
    retired.sign(&laptop, 11, Change::Remove { id: desk.public(), cascade: false }).unwrap();
    r.sign(&laptop, 11, Change::Remove { id: desk.public(), cascade: true }).unwrap();
    r.sign(&laptop, 12, Change::Admit { id: server.public(), name: "server".into() }).unwrap();
    // The thief signs as desk, dated from before the handover.
    let forge = |copy: &Roster| {
        let mut forged = copy.clone();
        let changes = [
            Change::Remove { id: laptop.public(), cascade: true },
            Change::Remove { id: server.public(), cascade: false },
            Change::Promote { id: server.public() },
        ];
        forged.log.extend(changes.map(|change| Signed::new(&copy.network, &desk, 9, change)));
        forged
    };
    r.merge(&forge(&r));
    assert_eq!((r.role(&laptop.public()), r.role(&server.public())), (Some(Role::Owner), Some(Role::Member)));
    assert!(r.is_removed(&desk.public()));
    // Removed without cascade, the old owner still can't remove the new one.
    retired.merge(&forge(&retired));
    assert_eq!(retired.role(&laptop.public()), Some(Role::Owner));
}

#[test]
fn two_admins_naming_a_laptop_at_once_still_get_distinct_names() {
    let (desk, server, laptop, other) = (key(), key(), key(), key());
    let (mut a, _root) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    a.sign(&desk, 2, Change::Admit { id: server.public(), name: "server".into() }).unwrap();
    a.sign(&desk, 3, Change::Promote { id: server.public() }).unwrap();
    let mut b = a.clone();
    // Each admits a "laptop" before hearing of the other's.
    a.sign(&desk, 4, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    b.sign(&server, 5, Change::Admit { id: other.public(), name: "Laptop".into() }).unwrap();
    assert!(a.merge(&b) && b.merge(&a));
    assert_eq!(a.members(), b.members());
    assert_eq!((a.name(&laptop.public()), a.name(&other.public())), ("laptop".into(), "Laptop-2".into()));
    assert_eq!(a.find("laptop-2"), Some(other.public()));
    // Removing the first laptop moves no one: the second stays laptop-2. A computer admitted later
    // may take the freed name.
    a.sign(&desk, 6, Change::Remove { id: laptop.public(), cascade: false }).unwrap();
    assert_eq!(a.name(&other.public()), "Laptop-2");
    let new = key();
    a.sign(&desk, 7, Change::Admit { id: new.public(), name: "laptop".into() }).unwrap();
    assert_eq!(a.name(&new.public()), "laptop");
}

#[test]
fn removal_tied_with_another_admission_keeps_the_surviving_name() {
    let (desk, server, laptop, other) = (key(), key(), key(), key());
    let (mut a, _) = Roster::create(&desk, "desk".into(), addr(&desk), 1);
    a.sign(&desk, 2, Change::Admit { id: server.public(), name: "server".into() }).unwrap();
    a.sign(&desk, 3, Change::Promote { id: server.public() }).unwrap();
    let mut b = a.clone();
    a.sign(&desk, 4, Change::Admit { id: laptop.public(), name: "laptop".into() }).unwrap();
    b.sign(&server, 5, Change::Admit { id: other.public(), name: "Laptop".into() }).unwrap();
    a.merge(&b);
    assert_eq!(a.name(&other.public()), "Laptop-2");

    // Separate signers can produce an admission and removal in the same millisecond.
    b.merge(&a);
    b.sign(&desk, 5, Change::Remove { id: laptop.public(), cascade: false }).unwrap();
    a.merge(&b);
    b.merge(&a);
    assert_eq!(a.members(), b.members());
    assert_eq!(a.name(&other.public()), "Laptop-2");
    assert_eq!(a.find("laptop-2"), Some(other.public()));
}
