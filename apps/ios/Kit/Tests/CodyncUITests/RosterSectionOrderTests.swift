import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@Test func sectionOrderPersistsPerComputerAndKeepsRenamesInPlace() {
    var order = RosterSectionOrder(data: Data())
    order.set(["", "Work", "Home"], on: "mac")
    order.set(["Home", "Work", ""], on: "other")
    order.rename("Work", to: "Work", on: "mac")
    order = RosterSectionOrder(data: order.data)
    #expect(order.saved(on: "mac") == ["", "Work", "Home"])
    #expect(order.saved(on: "other") == ["Home", "Work", ""])
    order.rename("Work", to: "Play", on: "mac")
    #expect(order.saved(on: "mac") == ["", "Play", "Home"])
    order.rename("Play", to: "Home", on: "mac")
    #expect(order.saved(on: "mac") == ["", "Home"])
}

@Test func sectionsMoveAcrossBotsWithoutMovingPinnedBots() throws {
    let bots = try [#"{"id":"pin","pinned":true}"#, #"{"id":"a","section":"Work"}"#, #"{"id":"b","section":"Work"}"#, #"{"id":"loose"}"#]
        .map { try JSONDecoder().decode(Bot.self, from: Data($0.utf8)) }
    func shown(_ names: [String], folded: Set<String> = []) -> [String] {
        RosterSection.lines(bots, folded: folded, order: names) { $0 }.compactMap {
            if case let .bot(bot) = $0 { bot.id } else { nil }
        }
    }
    #expect(shown(["", "Work"]) == ["pin", "loose", "a", "b"])
    #expect(shown(["Work", ""]) == ["pin", "a", "b", "loose"])
    #expect(shown(["", "Work"], folded: ["Work"]) == ["pin", "loose"])
}

@Test func mergingIntoANewGroupKeepsItsDisplayedPosition() {
    var order = RosterSectionOrder(data: Data())
    order.set(["Work", ""], on: "mac")
    let displayed = ComputerOrder(ids: order.saved(on: "mac")).orderedIDs(["Home", "Work", ""])
    order.set(displayed, on: "mac")
    order.rename("Work", to: "Home", on: "mac")
    #expect(order.saved(on: "mac") == ["", "Home"])
}
