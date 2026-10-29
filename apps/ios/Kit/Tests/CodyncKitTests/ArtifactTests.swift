import CryptoKit
import Foundation
import Testing
@testable import CodyncKit

private func artifactJSON(_ html: Data, id: String = "p1", updatedAt: Int64 = 5, title: String = "Weekly report") -> [String: Any] {
    ["id": id, "botId": "b1", "title": title, "size": html.count, "createdAt": 1, "updatedAt": updatedAt,
     "sha256": SHA256.hash(data: html).map { String(format: "%02x", $0) }.joined(), "entryId": "e9"]
}

@Test func parsesArtifactEventsAndCards() throws {
    let event = HostClient.parseEvent(Data(#"{"type":"artifact","artifact":{"id":"p1","botId":"b1","title":"Weekly report","size":22,"sha256":"ab","createdAt":1,"updatedAt":5,"entryId":"e9"}}"#.utf8))
    guard case let .artifact(artifact) = event else { Issue.record("artifact"); return }
    #expect(artifact.title == "Weekly report" && artifact.updatedAt == 5)
    #expect(artifact.replyQuote == ReplyQuote(id: "e9", author: "b1", text: "Artifact: Weekly report", artifact: "p1"))
    // An artifact event this app can't read is skipped, not a reason to fetch everything again.
    #expect(HostClient.parseEvent(Data(#"{"type":"artifact","artifact":{"id":"p1"}}"#.utf8)) == nil)

    let card = #"{"id":"e9","botId":"b1","seq":3,"rev":7,"kind":"agent","turn":1,"data":{"text":"Artifact: Weekly report","final":true,"artifact":{"id":"p1","title":"Weekly report","updatedAt":5}},"createdAt":1,"updatedAt":1}"#
    let entry = try JSONDecoder().decode(Entry.self, from: Data(card.utf8))
    #expect(entry.data.artifact == ArtifactRef(id: "p1", title: "Weekly report", updatedAt: 5))
    #expect(entry.replyQuote.artifact == "p1", "a reply to the card is about the artifact")

    let reply = #"{"id":"u1","botId":"b1","seq":4,"rev":8,"kind":"user","turn":2,"data":{"text":"Add totals","replyTo":{"id":"e9","author":"b1","text":"Artifact: Weekly report","artifact":"p1"}},"createdAt":1,"updatedAt":1}"#
    #expect(try JSONDecoder().decode(Entry.self, from: Data(reply.utf8)).data.replyTo?.artifact == "p1")
}

@Test func readsTheLibraryAndSkipsWhatItCantRead() throws {
    let json = #"{"items":[{"kind":"artifact","at":9,"artifact":{"id":"p1","botId":"b1","title":"Weekly report","size":1,"sha256":"ab","createdAt":1,"updatedAt":9}},{"kind":"file","at":8,"entryId":"e2","file":{"id":"f1","name":"r.csv","size":3,"sha256":"cd"}},{"kind":"attachment","at":7,"entryId":"e1","file":{"id":"u1","name":"a.png","size":4}},{"kind":"video","at":6},{"kind":"artifact","at":6,"artifact":{"id":"p2"}},{"kind":"file","at":5,"entryId":"e3","file":{"id":"f2","name":"x","size":1}}]}"#
    struct Res: Decodable { var items: [ChatLibraryItem.Wire] }
    let items = try JSONDecoder().decode(Res.self, from: Data(json.utf8)).items.compactMap(\.item)
    #expect(items.map(\.id) == ["artifact/p1", "file/e2/f1", "attachment/e1/u1"])
    guard case let .file(file, entryId) = items[1].kind else { Issue.record("file"); return }
    #expect(file.sha256 == "cd" && entryId == "e2" && items[1].at == 8)
}

/// Serves an artifact's HTML in `readArtifact` chunks; `republish` swaps in another version after the first chunk.
private actor ArtifactTransport: HostTransport {
    var html: Data
    var updatedAt: Int64 = 5
    var republish: Data?
    var damaged = false
    private(set) var offsets: [Int] = []

    init(_ html: Data, republish: Data? = nil, damaged: Bool = false) {
        self.html = html
        self.republish = republish
        self.damaged = damaged
    }

    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        #expect(method == "readArtifact")
        let request = try #require(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let offset = try #require(request["offset"] as? Int)
        offsets.append(offset)
        var artifact = artifactJSON(html, updatedAt: updatedAt)
        if damaged { artifact["sha256"] = "00" }
        let chunk = html.subdata(in: offset..<min(offset + FileDownload.chunkSize, html.count))
        if let next = republish {
            html = next
            updatedAt += 1
            republish = nil
        }
        return try JSONSerialization.data(withJSONObject: ["data": chunk.base64EncodedString(), "size": artifact["size"] ?? 0, "artifact": artifact])
    }

    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { AsyncThrowingStream { $0.finish() } }
    nonisolated func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }
}

@Test func readsAWholeArtifactInChunks() async throws {
    let html = Data(String(repeating: "<p>row</p>", count: FileDownload.chunkSize / 5).utf8)
    let transport = ArtifactTransport(html)
    let read = try await HostClient(transport: transport).readArtifactHTML(botId: "b1", artifactId: "p1")
    #expect(read.html == html && read.artifact.updatedAt == 5)
    #expect(await transport.offsets == [0, FileDownload.chunkSize])
}

@Test func aArtifactPublishedAgainMidReadIsReadFromItsStart() async throws {
    let first = Data(String(repeating: "a", count: FileDownload.chunkSize + 10).utf8)
    let second = Data(String(repeating: "b", count: FileDownload.chunkSize + 20).utf8)
    let transport = ArtifactTransport(first, republish: second)
    let read = try await HostClient(transport: transport).readArtifactHTML(botId: "b1", artifactId: "p1")
    #expect(read.html == second && read.artifact.updatedAt == 6)
    #expect(await transport.offsets == [0, FileDownload.chunkSize, 0, FileDownload.chunkSize])
}

@Test func aDamagedArtifactIsRefused() async {
    await #expect(throws: FileDownloadError.self) {
        _ = try await HostClient(transport: ArtifactTransport(Data("<h1>Hi</h1>".utf8), damaged: true)).readArtifactHTML(botId: "b1", artifactId: "p1")
    }
}
