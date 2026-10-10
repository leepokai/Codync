/// Structured conversation data added to a team-tool notice; its existing display fields remain unchanged.
public struct BotMessage: Codable, Hashable, Sendable {
    public var sourceBotId: String
    public var targetBotId: String
    public var text: String
    public var reply: String?
    public var detail: String?

    public init(sourceBotId: String, targetBotId: String, text: String, reply: String? = nil, detail: String? = nil) {
        self.sourceBotId = sourceBotId
        self.targetBotId = targetBotId
        self.text = text
        self.reply = reply
        self.detail = detail
    }
}
