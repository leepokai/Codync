import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// Uses the existing fake computer for sync, overriding only the voice session response.
private actor SearchRemote: RemoteTransport {
    let base = FakeRemote(.ready(.direct))
    let acknowledged: Bool?
    private(set) var requests: [Data] = []

    init(_ acknowledged: Bool?) { self.acknowledged = acknowledged }

    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        if method == "voiceStatus" {
            return Data("{\"providers\":[],\"googleSearch\":\(acknowledged == true)}".utf8)
        }
        guard method == "startCall" else { return try await base.call(method, body: body, timeout: timeout) }
        requests.append(body)
        var reply: [String: Any] = ["credential": "test-token", "instructions": "Relay", "tools": [["name": "bot_status", "description": "Status"]]]
        if let acknowledged { reply["googleSearch"] = acknowledged }
        return try JSONSerialization.data(withJSONObject: reply)
    }
    nonisolated func states() -> AsyncStream<LinkState> { base.states() }
    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { base.stream(request) }
    nonisolated func computerUpdates() -> AsyncStream<Computer> { base.computerUpdates() }
    nonisolated func mailboxEvents() -> AsyncStream<MailboxEvent> { base.mailboxEvents() }
    func enqueue(botId: String, text: String, clientNonce: String, replyTo: String?) async throws {
        try await base.enqueue(botId: botId, text: text, clientNonce: clientNonce, replyTo: replyTo)
    }
    func cancelQueued(clientNonce: String) async -> MailboxCancel { await base.cancelQueued(clientNonce: clientNonce) }
    func listQueued() async throws -> [QueuedItem] { try await base.listQueued() }
    func shutdown() async { await base.shutdown() }
}

extension CallRecoveryTests {
    @Test func geminiSetupKeepsBotToolsAndOnlyEnablesAcknowledgedSearch() async throws {
        for enabled in [nil, false, true] as [Bool?] {
            let (storage, suite) = context()
            let remote = SearchRemote(enabled)
            let store = BotStore(computer: randomComputer("Search test"), storage: storage) { _ in remote }
            store.setActive(true)
            let sockets = ScriptedSockets()
            let engine = GeminiLiveEngine(model: "gemini-live", voice: "Kore", link: CallLink(model: store, botId: "b1", call: UUID().uuidString, googleSearch: { enabled == true }),
                                          audio: ScriptedAudio()) { sockets.open($0) }
            defer { engine.end(); store.retire(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
            #expect(await until { await remote.base.subscribed })
            await remote.base.emit(#"{"type":"bot","bot":{"id":"b1","name":"Bot","rev":1,"lastAt":1}}"#)
            #expect(await until { store.connection == .online })
            await engine.start()
            let setup = try #require(sockets.last?.sent("setup").first?["setup"] as? [String: Any])
            let tools = try #require(setup["tools"] as? [[String: Any]])
            #expect(tools.contains { $0["googleSearch"] != nil } == (enabled == true))
            let functions = try #require(tools.first?["functionDeclarations"] as? [[String: Any]])
            #expect(functions.first?["name"] as? String == "bot_status")
            let request = try #require(await remote.requests.last)
            let fields = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
            #expect(fields["googleSearch"] as? Bool == (enabled == true))
            #expect(engine.searchResult == nil)
            sockets.last?.play(#"{"serverContent":{"outputTranscription":{"text":"A grounded answer."}}}"#)
            sockets.last?.play(#"{"serverContent":{"groundingMetadata":{"searchEntryPoint":{"renderedContent":"<p>Google suggestions</p>"}},"turnComplete":true}}"#)
            #expect(await until { engine.searchResult?.text == "A grounded answer." })
            #expect(engine.searchResult?.html == "<p>Google suggestions</p>")
            sockets.last?.play(#"{"serverContent":{"outputTranscription":{"text":"Next answer"},"turnComplete":true}}"#)
            #expect(await until { engine.searchResult == nil })
        }
    }

    @Test func requestingSearchFromAnOlderHostKeepsVoiceWorking() async throws {
        let remote = SearchRemote(nil)
        let client = HostClient(transport: remote)
        let session = try await client.startCall(botId: "b", call: "c", mode: "realtime", provider: .gemini, language: "en", googleSearch: true)
        #expect(!session.googleSearch)
        let request = try #require(await remote.requests.last)
        let fields = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        #expect(fields["googleSearch"] as? Bool == false)
    }
}
