import Foundation
import Testing
@testable import CodyncKit

@Test func shortcutBotsExcludeHiddenAndRemovedComputersAndKeepAccountScope() throws {
    let suite = "ShortcutBots.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: "alice", suite: suite)
    let other = SharedStore.Context(accountID: "bob", suite: suite)
    let computer = Vectors.shared.computer
    storage.computers = [computer]
    let bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b1","name":"Ada"}"#.utf8))
    var hidden = bot
    hidden.id = "hidden"
    hidden.hidden = true
    storage.bots = [BotSnapshot(computerId: computer.id, bot: bot),
                    BotSnapshot(computerId: computer.id, bot: hidden),
                    BotSnapshot(computerId: "removed", bot: bot)]
    let choices = ShortcutBotQuery.bots(in: storage)
    #expect(choices.count == 1)
    let choice = try #require(choices.first)
    #expect(choice.name == "Ada")
    #expect(choice.computerName == computer.name)
    #expect(choice.reference(in: storage)?.botId == "b1")
    other.computers = storage.computers
    other.bots = storage.bots
    #expect(choice.reference(in: other) == nil)
    #expect(ShortcutBotQuery.bots(in: other).first?.id != choice.id)
    storage.bots = []
    #expect(choice.reference(in: storage) == nil)
    storage.bots = [BotSnapshot(computerId: computer.id, bot: bot)]
    storage.computers = []
    #expect(choice.reference(in: storage) == nil)
}

@Test func shortcutLeadsComeFirstAndFollowTheComputerOrder() throws {
    let suite = "ShortcutLeads.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let storage = SharedStore.Context(accountID: "alice", suite: suite)
    let laptop = Vectors.shared.computer
    var mini = laptop
    mini.id = "mini"
    storage.computers = [laptop, mini]
    func bot(_ id: String, _ name: String, lead: Bool = false) throws -> Bot {
        try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"\#(id)","name":"\#(name)","lead":\#(lead)}"#.utf8))
    }
    let ada = BotSnapshot(computerId: laptop.id, bot: try bot("a", "Ada"))
    storage.bots = [ada]
    #expect(ShortcutBotQuery.lead(in: storage) == nil, "no lead: the action asks for a bot")
    storage.bots = [ada,
                    BotSnapshot(computerId: laptop.id, bot: try bot("l1", "Zed", lead: true)),
                    BotSnapshot(computerId: mini.id, bot: try bot("l2", "Codync", lead: true)),
                    BotSnapshot(computerId: "removed", bot: try bot("l3", "Gone", lead: true))]
    #expect(ShortcutBotQuery.bots(in: storage).map(\.name) == ["Codync", "Zed", "Ada"])
    #expect(ShortcutBotQuery.lead(in: storage)?.name == "Zed", "unarranged: the first computer saved")
    storage.computerOrder = ComputerOrder(ids: ["removed", mini.id, laptop.id])
    #expect(ShortcutBotQuery.lead(in: storage)?.name == "Codync", "the user's order, past a removed computer")
}
