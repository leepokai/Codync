import Foundation

public extension HostClient {
    /// `automatic`: the chat is on screen, not the user reading it. The host leaves one marked unread alone.
    func markRead(_ id: String, automatic: Bool = false) async throws {
        struct Body: Encodable { var botId: String; var automatic: Bool? }
        let _: Empty = try await call("markRead", Body(botId: id, automatic: automatic ? true : nil))
    }

    /// The chat back to unread: what was unread before its last read, or else its latest message.
    /// The bot as it is now: not `keptUnread` when there was nothing to mark.
    func markUnread(_ id: String) async throws -> Bot {
        struct Res: Decodable { var bot: Bot }
        let res: Res = try await call("markUnread", ["botId": id])
        return res.bot
    }
}
