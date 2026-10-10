import Foundation
import SwiftUI
import Testing
@testable import CodyncKit
@testable import CodyncUI

private actor DelayedFileTransport: HostTransport {
    private var pending: CheckedContinuation<Data, any Error>?
    var called: Bool { pending != nil }
    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        try await withCheckedThrowingContinuation { pending = $0 }
    }
    func release() { pending?.resume(returning: Data(#"{"data":"","size":0}"#.utf8)); pending = nil }
    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { AsyncThrowingStream { $0.finish() } }
    nonisolated func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }
}

@MainActor @Test func retiringFileDownloadsCannotExportALateChunk() async throws {
    let downloads = FileDownloads()
    let transport = DelayedFileTransport()
    let file = SharedFile(id: "file", name: "empty", size: 0, sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    let entry = Entry(id: "entry", seq: 1, botId: "bot", rev: 1, kind: "agent", turn: 1, data: EntryData(), createdAt: 0, updatedAt: 0)
    downloads.save(client: HostClient(transport: transport), entry: entry, file: file)
    for _ in 0..<100 {
        let called = await transport.called
        if called { break }
        try await Task.sleep(for: .milliseconds(10))
    }
    let called = await transport.called
    #expect(called)
    downloads.retire()
    await transport.release()
    try await Task.sleep(for: .milliseconds(30))
    #expect(downloads.export == nil)
    #expect(downloads.states.isEmpty)
}

/// Opt-in product screenshots using the actual card and text bubble views.
@MainActor @Test func renderGeneratedFileCards() throws {
    guard let directory = ProcessInfo.processInfo.environment["CODYNC_RENDER_DIR"] else { return }
    let suite = "CodyncFileCardRender.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let store = BotStore(computer: Computer(id: "render", name: "Computer", signKey: "", boxKey: "", cloud: nil), clientKind: "ios", storage: SharedStore.Context(accountID: "render", suite: suite)) { FakeRemote(.connecting) }
    defer { store.retire() }
    func entry(_ name: String, size: Int64) -> Entry {
        var data = EntryData(text: "\(name) (\(size) bytes)")
        data.final = true
        data.files = [SharedFile(id: name, name: name, size: size, sha256: String(repeating: "0", count: 64))]
        return Entry(id: name, seq: 1, botId: "bot", rev: 1, kind: "agent", turn: 1, data: data, createdAt: 0, updatedAt: 0)
    }
    for scheme in [ColorScheme.light, .dark] {
        let content = VStack(alignment: .leading, spacing: 14) {
            Text("Your files are ready.").padding(14).background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22))
            AgentBubble(entry: entry("sample-results.csv", size: 71_000), openTrace: {})
            AgentBubble(entry: entry("report.pdf", size: 1_250_000), openTrace: {})
            AgentBubble(entry: entry("binary-output-with-a-long-filename.dat", size: 2_000_000), openTrace: {})
            AgentBubble(entry: entry(".empty", size: 0), openTrace: {})
        }.padding(18).frame(width: 390).background(Palette.background).environment(store).environment(\.colorScheme, scheme)
        let renderer = ImageRenderer(content: content)
        renderer.scale = 2
        let image = try #require(renderer.uiImage?.pngData())
        try image.write(to: URL(fileURLWithPath: directory).appendingPathComponent("iphone-\(scheme == .dark ? "dark" : "light").png"))
    }
}
