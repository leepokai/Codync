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
