import Foundation
import Testing
@testable import CodyncKit

private actor LiveCallClientTransport: HostTransport {
    private(set) var methods: [String] = []
    private(set) var bodies: [Data] = []
    var response: Data
    let supportsLive: Bool
    init(_ response: String, supportsLive: Bool = true) {
        self.response = Data(response.utf8)
        self.supportsLive = supportsLive
    }
    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        methods.append(method)
        bodies.append(body)
        if method == "voiceStatus" {
            return Data((supportsLive ? #"{"providers":[],"supportsLive":true}"# : #"{"providers":[]}"#).utf8)
        }
        return response
    }
    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { AsyncThrowingStream { $0.finish() } }
    nonisolated func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }
}

@Test func liveSessionUsesTheHostAndReturnsOnlyTheAnswer() async throws {
    let transport = LiveCallClientTransport(#"{"sessionId":"live-session","sdp":"answer"}"#)
    let session = try await HostClient(transport: transport).startLiveCall(botId: "b", call: "c", model: "gpt-live-1",
        voice: "marin", language: "zh-Hans", sdp: "offer")
    #expect(session.sessionId == "live-session" && session.sdp == "answer")
    #expect(await transport.methods == ["voiceStatus", "startLiveCall"])
    let body = try #require(await transport.bodies.last)
    let json = try #require(JSONSerialization.jsonObject(with: body) as? [String: String])
    #expect(json["sdp"] == "offer" && json["callId"] == "c" && json["botId"] == "b")
    #expect(json["credential"] == nil)
}

@Test func liveEventsPreserveSessionIdentityAndNestedProviderEvents() async throws {
    let transport = LiveCallClientTransport(#"{"events":[{"type":"response.create"}]}"#)
    let client = HostClient(transport: transport)
    let commands = try await client.callLiveEvent(call: "c", sessionId: "s", event: Data(#"{"type":"response.event","event":{"type":"response.completed"}}"#.utf8))
    #expect(commands.count == 1)
    let body = try #require(await transport.bodies.last)
    let json = try #require(JSONSerialization.jsonObject(with: body) as? [String: Any])
    #expect(json["sessionId"] as? String == "s")
    #expect((json["event"] as? [String: Any])?["type"] as? String == "response.event")
}

@Test func modelDiscoveryAdvertisesLiveSupportWithoutChangingTheDefaults() async throws {
    let transport = LiveCallClientTransport(#"{"models":["gpt-live-1","gpt-realtime-2.1"],"transcribe":[],"speech":[],"defaults":{"realtime":"gpt-realtime-2.1","transcribe":"gpt-transcribe","speech":"gpt-4o-mini-tts"}}"#)
    let models = try await HostClient(transport: transport).voiceModels(.openAI)
    #expect(models.defaults?.realtime == "gpt-realtime-2.1")
    let body = try #require(await transport.bodies.last)
    let json = try #require(JSONSerialization.jsonObject(with: body) as? [String: Any])
    #expect(json["supportsLive"] as? Bool == true)
}

@Test func liveModelAccessTestDoesNotRequireARealtimeCredential() async throws {
    let transport = LiveCallClientTransport(#"{"validated":true}"#)
    let credential = try await HostClient(transport: transport).voiceSession(.openAI, model: "gpt-live-1", voice: "marin")
    #expect(credential.isEmpty)
}

@Test func incompleteLiveSessionAnswerIsRejected() async throws {
    let transport = LiveCallClientTransport(#"{"sessionId":"","sdp":""}"#)
    await #expect(throws: HostError.self) {
        try await HostClient(transport: transport).startLiveCall(botId: "b", call: "c", model: "gpt-live-1",
            voice: "marin", language: nil, sdp: "offer")
    }
}

@Test func oldHostsRejectLiveBeforeAnUnsupportedSessionRequest() async throws {
    let transport = LiveCallClientTransport(#"{"sessionId":"unused","sdp":"unused"}"#, supportsLive: false)
    await #expect(throws: HostError.self) {
        try await HostClient(transport: transport).startLiveCall(botId: "b", call: "c", model: "gpt-live-1",
            voice: "marin", language: nil, sdp: "offer")
    }
    #expect(await transport.methods == ["voiceStatus"])
}

@Test func aLiveModelSavedOnAnotherHostCannotUseOldHostsRealtimeTest() async throws {
    let transport = LiveCallClientTransport(#"{"credential":"must-not-mint"}"#, supportsLive: false)
    await #expect(throws: HostError.self) {
        try await HostClient(transport: transport).voiceSession(.openAI, model: "gpt-live-1", voice: "marin")
    }
    #expect(await transport.methods == ["voiceStatus"])
}
