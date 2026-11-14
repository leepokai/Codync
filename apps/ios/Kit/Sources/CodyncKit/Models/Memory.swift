import Foundation

/// One memory entry.
public struct MemoryEntry: Decodable, Identifiable, Hashable, Sendable {
    public var id: Int64
    /// The bot whose own layer holds it; nil for what all bots share.
    public var bot: String?
    /// active · archived
    public var state: String
    public var text: String
    /// user · bot · dream · imported
    public var author: String
    public var authorBot: String?
    public var pinned: Bool
    public var updatedAt: Int64

    public var archived: Bool { state == "archived" }
}

public struct MemoryLayer: Decodable, Hashable, Sendable {
    public var entries: [MemoryEntry]
    public var used: Int
    public var cap: Int
}

/// One dreaming run: what it added and archived (`undone` when an older app undid it).
public struct DreamRun: Decodable, Identifiable, Hashable, Sendable {
    public struct Change: Decodable, Hashable, Sendable {
        public var id: Int64
        public var text: String
    }
    /// A long-form file the run added a line to.
    public struct FileNote: Decodable, Hashable, Sendable {
        public var name: String
        public var shared: Bool
        public var version: Int64
    }
    public var id: Int64
    public var at: Int64
    public var undone: Bool
    public var added: [Change]
    public var archived: [Change]
    public var files: [FileNote]?
}

/// A bot's memory as its screen shows it.
public struct MemoryView: Decodable, Hashable, Sendable {
    public struct Project: Decodable, Hashable, Sendable {
        public var path: String
        public var text: String
    }
    /// Whether this bot uses what all bots share.
    public var shared: Bool
    public var yours: MemoryLayer
    public var bot: MemoryLayer
    public var dreams: [DreamRun]
    /// Claude Code's own notes for the bot's project folder, read-only.
    public var projectMemory: Project?
    /// The long-form files of both layers.
    public var files: MemoryFiles?
}

/// The text a single change of a composer's draft inserted, when it is long enough that the user pasted it
/// rather than typed it: typing, predictions and dictation add a word at a time.
public func pastedInsertion(from old: String, to new: String, minimum: Int = 24) -> String? {
    let old = Array(old), new = Array(new)
    guard new.count - old.count >= minimum else { return nil }
    var prefix = 0
    while prefix < old.count, old[prefix] == new[prefix] { prefix += 1 }
    var suffix = 0
    while suffix < old.count - prefix, old[old.count - 1 - suffix] == new[new.count - 1 - suffix] { suffix += 1 }
    let inserted = String(new[prefix..<(new.count - suffix)])
    return inserted.count >= minimum ? inserted : nil
}

extension HostClient {
    public func memory(botId: String) async throws -> MemoryView {
        try await call("memory", ["botId": botId])
    }
}
