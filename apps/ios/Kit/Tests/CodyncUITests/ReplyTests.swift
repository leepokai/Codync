import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

private func message(_ id: String, kind: String, final: Bool? = nil, text: String = "Hello") -> Entry {
    var data = EntryData(text: text)
    data.final = final
    data.author = kind == "agent" ? "b1" : nil
    return Entry(id: id, seq: 1, botId: "b1", rev: 1, kind: kind, turn: 1, data: data, createdAt: 1, updatedAt: 1)
}

@Test func onlyMessagesTheComputerHasCanBeQuoted() {
    #expect(message("u", kind: "user").isReplyable)
    #expect(message("a", kind: "agent", final: true).isReplyable)
    #expect(!message("a", kind: "agent", final: false).isReplyable, "narration is trace")
    #expect(!message("n", kind: "notice").isReplyable)
    #expect(!message("local-x", kind: "user").isReplyable, "not on the computer yet")
    #expect(message("a", kind: "agent", final: true).replyQuote == ReplyQuote(id: "a", author: "b1", text: "Hello"))
    #expect(message("u", kind: "user").replyQuote.author == "user")
}

@Test func aPartialQuoteStillDecodes() throws {
    let json = #"{"id":"e","botId":"b","seq":1,"rev":1,"kind":"user","turn":1,"data":{"text":"Why?","replyTo":{"id":"a"}},"createdAt":1,"updatedAt":1}"#
    let e = try JSONDecoder().decode(Entry.self, from: Data(json.utf8))
    #expect(e.data.replyTo?.id == "a")
    #expect(e.data.replyTo?.text == "")
}

@MainActor @Test func aReplySendsItsTargetAndShowsTheQuoteAtOnce() async throws {
    let suite = "ReplyTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: Computer(id: "reply", name: "Mac", signKey: ""), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"lastAt":1}}"#)
    #expect(await until { store.connection == .online })

    store.setComposerDraft("Why?", for: "b1")
    #expect(store.sendComposerDraft(to: "b1", replyTo: message("a", kind: "agent", final: true).replyQuote))
    #expect(store.chat("b1").last?.data.replyTo?.id == "a")
    #expect(store.composerDraft(for: "b1").isEmpty)
    #expect(await until { await fake.replyTos == ["a"] })
}
