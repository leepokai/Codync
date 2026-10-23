import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func reorderingKeepsPinnedBotsOnTop() throws {
    let suite = "RosterOrderTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    let store = BotStore(computer: Computer(id: UUID().uuidString, name: "Mac", signKey: "test"), storage: storage) { _ in
        FakeRemote(.connecting)
    }
    defer { store.retire() }
    for (id, pinned, lastAt) in [("p", true, 0), ("a", false, 3), ("b", false, 2), ("c", false, 1)] {
        let json = #"{"id":"\#(id)","pinned":\#(pinned),"lastAt":\#(lastAt)}"#
        store.bots[id] = try JSONDecoder().decode(Bot.self, from: Data(json.utf8))
    }
    let order = { store.roster.map(\.id) }
    #expect(order() == ["p", "a", "b", "c"]) // Never arranged: recent activity.
    store.reorder(order()) // A drop in place: nothing to save or send.
    #expect(store.bots.values.allSatisfy { $0.position == 0 })
    store.reorder(["p", "c", "a", "b"])
    #expect(order() == ["p", "c", "a", "b"])
    store.move("a", by: -1)
    #expect(order() == ["p", "a", "c", "b"])
    store.move("a", by: -1) // An unpinned bot stays below the pinned ones.
    #expect(order() == ["p", "a", "c", "b"])
    store.reorder(["c", "p", "a", "b"]) // Dropped above a pinned bot: still listed below it.
    #expect(order() == ["p", "c", "a", "b"])
}

/// A bot as the host sends it.
private func rosterBot(_ id: String, pinned: Bool = false, hidden: Bool = false, section: String? = nil, unread: Int = 0,
                       lastAt: Int = 0) throws -> Bot {
    var json: [String: Any] = ["id": id, "pinned": pinned, "hidden": hidden, "unread": unread, "lastAt": lastAt]
    if let section { json["section"] = section }
    return try JSONDecoder().decode(Bot.self, from: JSONSerialization.data(withJSONObject: json))
}

/// A store holding `bots` that never connects; `done` retires it and drops its defaults.
@MainActor private func rosterStore(_ bots: [Bot]) -> (store: BotStore, done: () -> Void) {
    let suite = "RosterOrderTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    let store = BotStore(computer: Computer(id: UUID().uuidString, name: "Mac", signKey: "test"), storage: storage) { _ in
        FakeRemote(.connecting)
    }
    for item in bots { store.bots[item.id] = item }
    return (store, {
        store.retire()
        UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite)
    })
}

@Test func sectionsListPinnedBotsThenSectionsByNameThenTheRest() throws {
    let roster = try [
        rosterBot("p", pinned: true, section: "Work"),
        rosterBot("w1", section: "Work", unread: 2),
        rosterBot("loose"),
        rosterBot("i", section: "İş"),
        rosterBot("h", section: "home"),
        rosterBot("w2", section: "Work", unread: 3),
    ]
    func shown(_ folded: Set<String>) -> [String] {
        RosterSection.lines(roster, folded: folded) { $0 }.map { line in
            switch line {
            case let .header(section): "\(section.folded ? "▸" : "▾") \(section.name) \(section.count) \(section.unread)"
            case .label: "BOTS"
            case let .bot(item): item.id
            }
        }
    }
    // A pinned bot shows only on top; its section counts the rest.
    #expect(shown([]) == ["p", "▾ home 1 0", "h", "▾ İş 1 0", "i", "▾ Work 2 5", "w1", "w2", "BOTS", "loose"])
    #expect(shown(["Work"]) == ["p", "▾ home 1 0", "h", "▾ İş 1 0", "i", "▸ Work 2 5", "BOTS", "loose"])
    // A section only pinned bots are in has no heading, and with no heading no label.
    #expect(RosterSection.lines([roster[0], roster[2]], folded: []) { $0 }.count == 2)
    // The label needs a bot in no section too.
    #expect(RosterSection.lines([roster[1]], folded: []) { $0 }.count == 2)
}

@Test func typedSectionNamesAreTrimmedAndCappedLikeTheHost() {
    #expect(RosterSection.name(typed: "") == nil)
    #expect(RosterSection.name(typed: " \n\t ") == nil)
    #expect(RosterSection.name(typed: "\u{1}\u{7F} ") == nil) // Control characters only.
    #expect(RosterSection.name(typed: "  Work ") == "Work")
    // 40 Unicode scalars, as the host counts them: "é" written as e + accent is two.
    let forty = String(repeating: "e\u{301}", count: 20)
    #expect(RosterSection.limited(forty) == forty)
    #expect(RosterSection.limited(forty + "x") == forty)
    #expect(RosterSection.limited("Work") == "Work")
}

@Test func aFoldedSectionRenamedOrRemovedHereTakesItsFoldAlong() {
    #expect(RosterSection.folds(["Home", "Work"], renaming: "Work", to: "Jobs") == ["Home", "Jobs"])
    #expect(RosterSection.folds(["Home"], renaming: "Work", to: "Jobs") == ["Home"]) // Unfolded: nothing changes.
    #expect(RosterSection.folds(["Home"], renaming: "Work", to: "Home") == ["Home"]) // Never unfolds the target.
    #expect(RosterSection.folds(["Home", "Work"], renaming: "Work", to: nil) == ["Home"]) // Removed: its name goes.
    #expect(RosterSection.folds(["Work"], renaming: "Work", to: "Home") == ["Home"]) // A merge folds the section it joins.
    #expect(RosterSection.folds(["Home", "Work"], renaming: "Work", to: "Home") == ["Home"])
}

@MainActor @Test func sectionMenuListsEverySectionOnTheComputer() throws {
    let (store, done) = try rosterStore([
        rosterBot("a", section: "Work"), rosterBot("p", pinned: true, section: "Pinned"),
        rosterBot("h", hidden: true, section: "Hidden"), rosterBot("b", section: "Work"), rosterBot("c"),
    ])
    defer { done() }
    #expect(store.sections == ["Hidden", "Pinned", "Work"]) // Hidden and pinned bots keep theirs.
}

@MainActor @Test func sectionMenusNeedAHostThatKnowsSections() throws {
    let (store, done) = rosterStore([])
    defer { done() }
    #expect(!store.supportsSections) // The host hasn't answered yet.
    store.hostVersion = HostVersion(version: "2.23.0")
    #expect(!store.supportsSections)
    store.hostVersion = HostVersion(version: "2.24.0")
    #expect(store.supportsSections)
    store.hostVersion = HostVersion(version: "2.24.1-dev")
    #expect(store.supportsSections)
}

@MainActor @Test func sectionChangesShowAtOnceAndUndoWhenRefused() async throws {
    let (store, done) = try rosterStore([rosterBot("a"), rosterBot("b", section: "Work"), rosterBot("c", section: "Home")])
    defer { done() }
    store.connection = .unauthorized("No access") // Every call fails at once.
    store.setSection(try #require(store.bots["a"]), to: "Work")
    #expect(store.bots["a"]?.section == "Work")
    #expect(await until { store.bots["a"]?.section == nil })
    #expect(store.lastError != nil)
    store.renameSection(from: "Work", to: "Home") // Onto a name in use: the two merge.
    #expect(store.sections == ["Home"])
    #expect(await until { store.bots["b"]?.section == "Work" })
    #expect(store.bots["c"]?.section == "Home") // Already in "Home": the undo leaves it.
    store.renameSection(from: "Home", to: nil)
    #expect(store.bots["c"]?.section == nil)
    #expect(await until { store.bots["c"]?.section == "Home" })
}

@MainActor @Test func botsMoveOnlyAmongTheirPinStateAndSection() throws {
    let (store, done) = try rosterStore([
        rosterBot("p1", pinned: true, section: "Work", lastAt: 9), rosterBot("p2", pinned: true, lastAt: 8),
        rosterBot("w1", section: "Work", lastAt: 7), rosterBot("a", lastAt: 6),
        rosterBot("w2", section: "Work", lastAt: 5), rosterBot("b", lastAt: 4),
    ])
    defer { done() }
    let order = { store.roster.map(\.id) }
    let p1 = try #require(store.bots["p1"]), p2 = try #require(store.bots["p2"])
    let w1 = try #require(store.bots["w1"]), w2 = try #require(store.bots["w2"]), a = try #require(store.bots["a"])
    #expect(p1.isRosterPeer(of: p2)) // Pinned bots share the top, whatever their sections.
    #expect(!p1.isRosterPeer(of: w1))
    #expect(w1.isRosterPeer(of: w2))
    #expect(!w1.isRosterPeer(of: a)) // Another section: none.
    #expect(order() == ["p1", "p2", "w1", "a", "w2", "b"])
    store.move("w2", by: -1) // Past "a", which is in no section: above w1.
    #expect(order() == ["p1", "p2", "w2", "a", "w1", "b"])
    store.move("w2", by: -1) // Already first in Work.
    #expect(order() == ["p1", "p2", "w2", "a", "w1", "b"])
    store.drop("a", in: ["p1", "p2", "w2", "b", "a", "w1"]) // Dropped below "b", among Work's bots.
    #expect(order() == ["p1", "p2", "w2", "b", "w1", "a"])
    store.drop("w1", in: ["w1", "p1", "p2", "w2", "b", "a"]) // Above the pinned bots: first of Work.
    #expect(order() == ["p1", "p2", "w1", "b", "w2", "a"])
    store.drop("p1", in: ["p2", "p1", "w1", "b", "w2", "a"])
    #expect(order() == ["p2", "p1", "w1", "b", "w2", "a"])
    store.drop("w1", in: ["p2", "p1", "w1", "b", "a"]) // A bot of its section isn't on screen: nothing moves.
    #expect(order() == ["p2", "p1", "w1", "b", "w2", "a"])
}
