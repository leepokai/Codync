import Foundation

// Voice calls in a chat (host/src/chat/calls): what a call sends the bot, and the bot's replies while it lasts,
// carry its id; the chat shows them only in the call's conversation, which its line opens.

/// What a client needs to run a call's voice, from the computer (`startCall`).
public struct CallSession: Sendable {
    /// A realtime provider's short-lived credential.
    public var credential: String?
    /// The voice's instructions: its role, then the background.
    public var instructions: String
    /// The voice's tools, a JSON array of `{name, description, parameters}` (JSON Schema).
    public var tools: Data

    init(json data: Data) throws {
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let instructions = object["instructions"] as? String else {
            throw HostError.http(0, "The computer sent no call instructions.")
        }
        credential = object["credential"] as? String
        self.instructions = instructions
        tools = try JSONSerialization.data(withJSONObject: object["tools"] as? [Any] ?? [])
    }
}

/// One message of a call's conversation.
public struct CallMessage: Hashable, Sendable {
    public enum Speaker: Sendable { case user, voice, voiceToBot, bot }
    public var speaker: Speaker
    public var text: String
}

public extension Array where Element == Entry {
    /// Without what voice calls carried: the messages of a call that has its line, or of a call this device is on.
    /// One that failed to send stays, to be sent again.
    func hidingCalls(live: Set<String>) -> [Entry] {
        let ended = Set(compactMap { $0.kind == "notice" ? $0.data.call : nil })
        return filter { e in
            guard e.kind != "notice", e.data.status != "failed", let call = e.data.call else { return true }
            return !ended.contains(call) && !live.contains(call)
        }
    }

    /// A call's conversation, oldest first: what was said (the voice's transcript) and the messages it carried.
    /// Phone and computer clocks are merged as they are.
    func callConversation(_ line: Entry) -> [CallMessage] {
        guard let call = line.data.call else { return [] }
        var said: [(at: Int64, message: CallMessage)] = (line.data.transcript ?? []).map {
            ($0.at ?? line.createdAt, CallMessage(speaker: $0.from == "user" ? .user : .voice, text: $0.text))
        }
        for e in self where e.data.call == call && (e.kind == "user" || (e.kind == "agent" && e.data.final == true)) {
            let speaker: CallMessage.Speaker = e.kind == "agent" ? .bot : e.data.from == "voice" ? .voiceToBot : .user
            said.append((e.createdAt, CallMessage(speaker: speaker, text: e.data.text ?? "")))
        }
        return said.enumerated().sorted { ($0.element.at, $0.offset) < ($1.element.at, $1.offset) }.map(\.element.message)
    }
}
