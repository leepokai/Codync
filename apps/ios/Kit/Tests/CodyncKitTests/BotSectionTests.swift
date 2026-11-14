import Foundation
import Testing
@testable import CodyncKit

/// Answers every call with `{}` and keeps each method's last body.
private actor Recorder: HostTransport {
    private(set) var bodies: [String: Data] = [:]

    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        bodies[method] = body
        return Data("{}".utf8)
    }

    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { AsyncThrowingStream { $0.finish() } }
    nonisolated func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }
}

private func json(_ data: Data?) throws -> [String: Any] {
    let data = try #require(data)
    return try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
}

@Test func botSectionDecodesAndSavingSettingsLeavesItAlone() throws {
    let bot = try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b","section":"Work"}"#.utf8))
    #expect(bot.section == "Work")
    #expect(try JSONDecoder().decode(Bot.self, from: Data(#"{"id":"b"}"#.utf8)).section == nil) // An older host.
    let body = try json(JSONEncoder().encode(BotUpdate(draft: BotDraft(bot))))
    #expect(body["section"] == nil)
}

@Test func sectionCallsSendOnlyTheSectionAndNullToClear() async throws {
    let recorder = Recorder()
    let client = HostClient(transport: recorder)
    try await client.setSection("b", to: "Work")
    var sent = try json(await recorder.bodies["updateBot"])
    #expect(sent["id"] as? String == "b")
    #expect(sent["section"] as? String == "Work")
    #expect(sent.count == 2)
    try await client.setSection("b", to: nil)
    sent = try json(await recorder.bodies["updateBot"])
    #expect(sent["section"] is NSNull)
    try await client.renameSection(from: "Work", to: "Home")
    sent = try json(await recorder.bodies["renameSection"])
    #expect(sent["from"] as? String == "Work")
    #expect(sent["to"] as? String == "Home")
    try await client.renameSection(from: "Home", to: nil)
    sent = try json(await recorder.bodies["renameSection"])
    #expect(sent["to"] is NSNull)
}

@Test func setLeadSendsOnlyTheBot() async throws {
    let recorder = Recorder()
    try await HostClient(transport: recorder).setLead("b")
    #expect(try json(await recorder.bodies["setLead"]) as? [String: String] == ["botId": "b"])
}
