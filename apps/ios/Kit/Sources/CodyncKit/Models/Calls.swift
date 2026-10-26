import Foundation

// Voice calls in a chat (host/src/chat/calls): what a call sends the bot, and the bot's replies while it lasts,
// carry its id; the chat shows them only in the call's conversation, which its line opens.

/// What a client needs to run a call's voice, from the computer (`startCall`).
public struct CallSession: Sendable {
    /// A realtime provider's short-lived credential.
    public var credential: String?
    /// Confirmed by the host; false when talking to an older host.
    public var googleSearch: Bool
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
        googleSearch = object["googleSearch"] as? Bool ?? false
        self.instructions = instructions
        tools = try JSONSerialization.data(withJSONObject: object["tools"] as? [Any] ?? [])
    }
}

/// Something the computer tells a call's voice while the call lasts: the bot's reply, a notice (an approval
/// waiting, a failure) or what the bot is doing.
public struct CallCue: Decodable, Sendable, Equatable {
    public enum Kind: String, Decodable, Sendable { case reply, notice, progress }
    /// The call's id.
    public var call: String
    public var kind: Kind
    /// Ready to say: worded for the call's voice, and in a group with who said it.
    public var text: String
    /// Rises with each cue of the call: one heard already (replayed after a reconnect) is skipped.
    public var seq: Int64
}

/// One message of a call's conversation.
public struct CallMessage: Hashable, Sendable {
    public enum Speaker: Sendable { case user, voice, voiceToBot, bot }
    public var speaker: Speaker
    public var text: String
}

public extension Array where Element == Entry {
    /// Without what voice calls carry, whichever device the call is on: it shows behind the call's line, which the
    /// computer writes when the call ends or its app went away. One that failed to send stays, to be sent again, and a
    /// file or an artifact the bot sent, to be opened.
    func hidingCalls() -> [Entry] {
        filter { e in
            e.kind == "notice" || e.data.call == nil || e.data.status == "failed" || e.data.files?.isEmpty == false
                || e.data.artifact != nil
        }
    }

    /// With a call's own entries (`callEntries`), which the chat may not hold: each entry once, the chat's copy first.
    func withCallEntries(_ fetched: [Entry]) -> [Entry] {
        let held = Set(map(\.id))
        return self + fetched.filter { !held.contains($0.id) }
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
