/// Structured conversation data added to a team-tool notice; its existing display fields remain unchanged.
public struct BotMessage: Codable, Hashable, Sendable {
    public var sourceBotId: String
    public var targetBotId: String
    public var text: String
    public var reply: String?
    public var detail: String?
    /// The peer's name when it is on another of the user's computers (its id is `name@computer`).
    public var peerName: String?

    public init(sourceBotId: String, targetBotId: String, text: String, reply: String? = nil, detail: String? = nil,
                peerName: String? = nil) {
        self.sourceBotId = sourceBotId
        self.targetBotId = targetBotId
        self.text = text
        self.reply = reply
        self.detail = detail
        self.peerName = peerName
    }
}
