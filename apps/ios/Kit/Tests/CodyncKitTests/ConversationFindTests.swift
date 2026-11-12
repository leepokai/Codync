import Testing
@testable import CodyncKit

private typealias Match = ConversationFind.Match

@Test func findMatchesIgnoreCaseAndCountEachPlaceOnce() {
    let messages = [(id: "a", text: "Week week WEEK"), (id: "b", text: "aaaa"), (id: "c", text: "nothing")]
    #expect(ConversationFind.matches(in: messages, query: "week").map(\.id) == ["a", "a", "a"])
    #expect(ConversationFind.matches(in: messages, query: "aa") == [Match(id: "b", occurrence: 0), Match(id: "b", occurrence: 1)])
    #expect(ConversationFind.matches(in: messages, query: "  ").isEmpty)
}

@Test func findStepsAroundAndKeepsItsMatchAsEarlierMessagesLoad() {
    var find = ConversationFind()
    #expect(find.label([]) == "", "nothing typed, nothing to say")
    find.type("needle")
    #expect(find.label([]) == L10n.tr("No results"))

    var matches = ConversationFind.matches(in: [(id: "x", text: "needle"), (id: "y", text: "needle, needle")], query: "needle")
    #expect(find.label(matches) == "1/3")
    find.step(forward: false, in: matches)
    #expect(find.label(matches) == "3/3", "back from the first wraps to the last")
    find.step(forward: true, in: matches)
    find.step(forward: true, in: matches)
    #expect(find.label(matches) == "2/3")

    // An earlier message loads above: the same match stays shown, further down the count.
    matches = ConversationFind.matches(in: [(id: "w", text: "a needle"), (id: "x", text: "needle"), (id: "y", text: "needle, needle")],
                                       query: "needle")
    #expect(find.label(matches) == "3/4")

    find.type("needles")
    #expect(find.shown == nil, "a new text starts from its first match")
}

@Test func findHoldsItsFirstMatchAsEarlierMatchesLoad() {
    var find = ConversationFind()
    find.type("needle")
    find.pin(ConversationFind.matches(in: [(id: "x", text: "needle")], query: "needle"))
    let loaded = ConversationFind.matches(in: [(id: "w", text: "a needle"), (id: "x", text: "needle")], query: "needle")
    #expect(find.label(loaded) == "2/2", "the match shown before stays shown")
    find.pin(loaded)
    #expect(find.shown?.id == "x", "pinning again keeps it")
}

@Test func findReportsEachSearchedTextOnceAndNeverTheText() {
    var find = ConversationFind()
    #expect(find.searched(found: false) == nil, "nothing typed")
    find.type("plan")
    #expect(find.searched(found: true) == .conversationFindSearched(found: true))
    #expect(find.searched(found: true) == nil)
    find.type("plans")
    #expect(find.searched(found: false) == .conversationFindSearched(found: false))
    #expect(AnalyticsEvent.conversationFindSearched(found: false).name == "conversation_find_searched")
    #expect(AnalyticsEvent.conversationFindSearched(found: false).properties == ["found": .bool(false)])
    #expect(AnalyticsEvent.conversationFindOpened.properties.isEmpty)
}
