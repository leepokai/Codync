import Foundation

// MARK: - Remote screen

public extension HostClient {
    func screenStatus() async throws -> ScreenState { try await call("screenStatus") }

    /// `failedAttempts`: tries that ended without a picture, so the next one can go the other way (`ScreenRoute`).
    func screenPrepare(failedAttempts: Int = 0) async throws -> ScreenConnection {
        let relay = if let channel = transport as? ChannelTransport {
            try await channel.screenRelayRequired(failedAttempts: failedAttempts)
        } else {
            false
        }
        return try await call("screenPrepare", ["relay": relay, "trickle": true])
    }

    /// Whether a screen attempt that couldn't even start should try the other way: Cloudflare first has one to try.
    func screenFallsBack() async -> Bool {
        await (transport as? ChannelTransport)?.prefersCloudflare ?? false
    }

    /// Sends the offer using the prepared session’s negotiated ICE mode.
    /// Trickle negotiations use a fresh session for every connection attempt.
    func screenOffer(sdp: String, session: String?, display: UInt32?) async throws -> ScreenAnswer {
        struct Body: Encodable { var sdp: String; var session: String?; var display: UInt32? }
        return try await call("screenOffer", Body(sdp: sdp, session: session, display: display), timeout: 30)
    }

    func screenClose(session: String) async throws {
        let _: Empty = try await call("screenClose", ["session": session])
    }

    /// Takes control from bots (they can still look) or hands it back.
    func screenTakeover(_ on: Bool) async throws -> ScreenState {
        try await call("screenTakeover", ["on": on])
    }
}

public struct ScreenAnswer: Codable, Sendable {
    public var session: String
    public var sdp: String
}
