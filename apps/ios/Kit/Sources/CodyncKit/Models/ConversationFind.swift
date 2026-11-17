import Foundation

/// Find in the open conversation: the text searched, where it occurs in the messages loaded, the match shown and
/// stepping between matches. The same on every client (desktop ⌘F, terminal `/`).
public struct ConversationFind: Equatable, Sendable {
    /// One place the text occurs: the message (its row id) and how many earlier matches that message holds.
    public struct Match: Equatable, Sendable {
        public let id: String
        public let occurrence: Int
    }

    public var query = ""
    /// The match shown; nil shows the first. Kept by identity, so earlier messages loading above don't move it.
    public var shown: Match?
    /// The text last reported as searched, so each one counts once.
    private var reported = ""

    public init() {}

    /// Every place `query` occurs in `messages` (row id and text, top to bottom), ignoring case; overlapping places count once.
    public static func matches(in messages: [(id: String, text: String)], query: String) -> [Match] {
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty else { return [] }
        return messages.flatMap { message in
            var found: [Match] = []
            var rest = message.text.startIndex..<message.text.endIndex
            while let range = message.text.range(of: query, options: .caseInsensitive, range: rest) {
                found.append(Match(id: message.id, occurrence: found.count))
                rest = range.upperBound..<message.text.endIndex
            }
            return found
        }
    }

    /// Where the shown match is among `matches` (nil when there are none).
    public func index(in matches: [Match]) -> Int? {
        guard !matches.isEmpty else { return nil }
        return shown.flatMap(matches.firstIndex(of:)) ?? 0
    }

    /// What the bar's count reads: `2/7`, nothing before anything was typed, `No results` when nothing matched.
    public func label(_ matches: [Match]) -> String {
        if query.trimmingCharacters(in: .whitespaces).isEmpty { return "" }
        guard let index = index(in: matches) else { return L10n.tr("No results") }
        return "\(index + 1)/\(matches.count)"
    }

    /// Holds the first of `matches` as the shown one, once there is one, so matches loading above don't move it.
    public mutating func pin(_ matches: [Match]) {
        if shown == nil { shown = matches.first }
    }

    /// The next or previous match, wrapping around.
    public mutating func step(forward: Bool, in matches: [Match]) {
        guard let index = index(in: matches) else { return }
        shown = matches[(index + (forward ? 1 : matches.count - 1)) % matches.count]
    }

    /// A new text starts again from its first match.
    public mutating func type(_ text: String) {
        query = text
        shown = nil
    }

    /// `conversation_find_searched` once per text (when it settles, is stepped through or the bar closes): whether it
    /// found anything, never the text.
    public mutating func searched(found: Bool) -> AnalyticsEvent? {
        guard !query.trimmingCharacters(in: .whitespaces).isEmpty, reported != query else { return nil }
        reported = query
        return .conversationFindSearched(found: found)
    }
}
