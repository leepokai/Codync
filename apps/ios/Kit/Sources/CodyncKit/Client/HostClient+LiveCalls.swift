import Foundation

/// A GPT-Live WebRTC answer. The provider key and session configuration stay on the host.
public struct LiveCallSession: Decodable, Sendable {
    public var sessionId: String
    public var sdp: String
}

public extension HostClient {
    func startLiveCall(botId: String, call: String, model: String, voice: String,
                       language: String?, sdp: String) async throws -> LiveCallSession {
        try await requireLiveSupport()
        struct Body: Encodable { var botId, callId, model, voice: String; var language: String?; var sdp: String }
        let session: LiveCallSession = try await self.call("startLiveCall", Body(botId: botId, callId: call, model: model, voice: voice,
                                                       language: language, sdp: sdp), timeout: 45)
        guard !session.sessionId.isEmpty, !session.sdp.isEmpty else {
            throw HostError.http(0, "The computer sent no GPT-Live session answer.")
        }
        return session
    }

    internal func requireLiveSupport() async throws {
        guard try await voiceStatus().supportsLive == true else {
            throw HostError.http(0, "Update Codync on the computer to use GPT-Live.")
        }
    }

    /// Only provider events reach this endpoint; the host owns transcript grouping and delegated tools.
    func callLiveEvent(call: String, sessionId: String, event: Data) async throws -> [Data] {
        let value = try JSONSerialization.jsonObject(with: event)
        let body = try JSONSerialization.data(withJSONObject: ["callId": call, "sessionId": sessionId, "event": value])
        return try await liveCommands("callLiveEvent", body: body)
    }

    func callLiveUpdate(call: String, sessionId: String, kind: CallCue.Kind, text: String) async throws -> [Data] {
        let body = try JSONSerialization.data(withJSONObject: ["callId": call, "sessionId": sessionId,
                                                               "kind": kind.rawValue, "text": text])
        return try await liveCommands("callLiveUpdate", body: body)
    }

    private func liveCommands(_ method: String, body: Data) async throws -> [Data] {
        let data = try await transport.call(method, body: body, timeout: 60)
        guard let object = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let events = object["events"] as? [[String: Any]] else {
            throw HostError.http(0, "The computer sent unreadable Live events.")
        }
        return try events.map { try JSONSerialization.data(withJSONObject: $0) }
    }
}
