import Foundation

/// "Earlier context was summarized": the harness's summary, when it gave one. Every field is optional so a newer
/// host's line still decodes.
public struct Compaction: Codable, Hashable, Sendable {
    public var summary: String?
    /// The host started it, ahead of time; otherwise the harness did when its context filled.
    public var byHost: Bool?
}

/// What a new conversation carries when the bot's own chat starts over: editable until the user's next message
/// there sends it.
public struct HandoffCard: Codable, Hashable, Sendable {
    /// draft · sent · kept
    public var status: String?
    /// cold · away · full
    public var reason: String?
    public var text: String?

    public var isDraft: Bool { status == "draft" }
}

extension HostClient {
    /// Saves the user's version of a handoff that hasn't been sent.
    public func editHandoff(entryId: String, text: String) async throws {
        struct Body: Encodable { var entryId: String; var text: String }
        let _: Empty = try await call("editHandoff", Body(entryId: entryId, text: text))
    }

    /// Carries on with the old conversation instead of starting over.
    public func keepConversation(entryId: String) async throws {
        let _: Empty = try await call("keepConversation", ["entryId": entryId])
    }
}
