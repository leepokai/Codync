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

@Test func onlyASidewaysDragTheMessagesWayStartsASwipe() {
    #expect(ReplySwipe.begins(CGPoint(x: 124, y: 0), .right, startX: 100))
    #expect(ReplySwipe.begins(CGPoint(x: -300, y: 40), .left, startX: 100))
    #expect(!ReplySwipe.begins(CGPoint(x: -300, y: 40), .right, startX: 100), "the other way")
    #expect(!ReplySwipe.begins(CGPoint(x: 200, y: 260), .right, startX: 100), "mostly vertical stays the scroll")
    #expect(!ReplySwipe.begins(CGPoint(x: 200, y: 0), .right, startX: 10), "the left edge stays the back swipe")
    #expect(ReplySwipe.begins(CGPoint(x: -200, y: 0), .left, startX: 10), "own messages can still swipe at the edge")
}

@Test func aSwipedMessageFollowsTheFingerThenResistsPastTheThreshold() {
    #expect(ReplySwipe.travel(40, .right) == 40)
    #expect(ReplySwipe.travel(-40, .left) == 40)
    #expect(ReplySwipe.travel(ReplySwipe.threshold + 40, .right) == ReplySwipe.threshold + 10)
    #expect(ReplySwipe.travel(-30, .right) == 0, "never the other way")
    #expect(ReplySwipe.shown(80, gap: 30) == 30, "the bubble stops at its column's edge")
    #expect(ReplySwipe.shown(40, gap: 100) == 40)
    #expect(ReplySwipe.shown(80, gap: 0) == 0, "a full-width bubble stays put but its travel still replies")
    #expect(ReplySwipe.shown(80, gap: -5) == 0)
    #expect(AnalyticsEvent.replySwiped.name == "reply_swiped")
    #expect(AnalyticsEvent.replySwiped.properties.isEmpty)
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

@Test func aSelectedPartStandsForTheMessageInItsQuote() throws {
    let answer = message("a", kind: "agent", final: true)
    let quote = answer.replyQuote(part: "  the risky step \n")
    #expect(quote.id == "a")
    #expect(quote.text == "the risky step")
    #expect(quote.selectedPart == "the risky step")
    #expect(answer.replyQuote(part: " \n").part == nil, "nothing selected quotes the whole message")
    #expect(answer.replyQuote.selectedPart == nil)
    #expect(answer.replyQuote(part: String(repeating: "x", count: 5000)).text.count == 4000)
    let stored = #"{"id":"a","author":"b","text":"the risky step","part":true}"#
    #expect(try JSONDecoder().decode(ReplyQuote.self, from: Data(stored.utf8)).selectedPart == "the risky step")
}

@MainActor @Test func aReplyToASelectedPartSendsThatPart() async throws {
    let suite = "ReplyTests.\(UUID().uuidString)"
    let storage = SharedStore.Context(accountID: UUID().uuidString, suite: suite)
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let fake = FakeRemote(.ready(.direct))
    let store = BotStore(computer: Computer(id: "reply-part", name: "Mac", signKey: ""), storage: storage) { _ in fake }
    defer { store.retire() }
    store.setActive(true)
    #expect(await until { await fake.subscribed })
    await fake.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"lastAt":1}}"#)
    #expect(await until { store.connection == .online })

    store.setComposerDraft("Why?", for: "b1")
    #expect(store.sendComposerDraft(to: "b1", replyTo: message("a", kind: "agent", final: true).replyQuote(part: "the risky step")))
    #expect(store.chat("b1").last?.data.replyTo?.text == "the risky step")
    #expect(await until { await fake.replyParts == ["the risky step"] })
    #expect(await fake.replyTos == ["a"])
}

@MainActor @Test func aMessagesTextOffersSelectTextRightAfterCopy() {
    let answer = message("a", kind: "agent", final: true)
    let reactions = ReactionPick(emoji: [], chosen: []) { _ in }
    let menu = MessageMenu.of(answer, reactions: reactions, reply: {}, selectText: {}, more: [MenuItem("Show what it did") {}])
    // A voice note's Download follows Select Text, as on Android.
    #expect(menu.textItems(more: [MenuItem("Download") {}]).map(\.title) == ["Copy", "Select Text", "Download", "Reply", "Show what it did"])
    // A picture's or a file's menu has none, nor the viewer's.
    #expect(menu.items(copy: [MenuItem("Copy image") {}]).map(\.title) == ["Copy image", "Reply", "Show what it did"])
    #expect(menu.inViewer(reactions: reactions) {}.textItems().map(\.title) == ["Copy", "Reply", "Show what it did"])
}

@Test func selectTextGoesWhereReplyDoesOnAMessageWithTextOfItsOwn() {
    let answer = message("a", kind: "agent", final: true)
    #expect(MessageMenu.offersSelectText(answer, replyable: true))
    #expect(!MessageMenu.offersSelectText(answer, replyable: false))
    #expect(!MessageMenu.offersSelectText(message("f", kind: "user", text: " \n"), replyable: true), "files only")
}
