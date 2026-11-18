import Foundation
import Testing
@testable import CodyncKit

@Test func ownFolderDraftDoesNotReuseAnotherBotsDirectory() throws {
    let bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"one","name":"One","cwd":"/data/bots/one/workspace","managedWorkspace":true}"#.utf8))
    let draft = BotDraft(bot)
    #expect(draft.cwd.isEmpty)
    #expect(bot.folderName == L10n.tr("Own folder"))
    let encoded = try JSONEncoder().encode(draft)
    let object = try #require(JSONSerialization.jsonObject(with: encoded) as? [String: Any])
    #expect(object["cwd"] as? String == "")
}

@Test func existingProjectFolderRemainsExplicit() throws {
    let bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"one","name":"One","cwd":"/projects/app"}"#.utf8))
    #expect(!bot.managedWorkspace)
    #expect(BotDraft(bot).cwd == "/projects/app")
}
