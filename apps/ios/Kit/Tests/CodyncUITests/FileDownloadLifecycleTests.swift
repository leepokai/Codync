import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

/// Pauses after the real writer has verified and moved a completed file.
private actor ControlledDownloadWriter: FileDownloadWriting {
    private let writer = FileDownload()
    private let directory: URL
    private var pauseCompletedFile: Bool
    private var completion: CheckedContinuation<Void, Never>?
    private(set) var pausedURL: URL?

    init(directory: URL, pauseCompletedFile: Bool) {
        self.directory = directory
        self.pauseCompletedFile = pauseCompletedFile
    }

    func save(client: HostClient, entryId: String, file: SharedFile,
              directory: URL, progress: @Sendable (Int64) async -> Void) async throws -> URL {
        let url = try await writer.save(client: client, entryId: entryId, file: file,
                                        directory: self.directory, progress: progress)
        if pauseCompletedFile {
            pauseCompletedFile = false
            pausedURL = url
            await withCheckedContinuation { completion = $0 }
        }
        return url
    }

    func releaseCompletedFile() { completion?.resume(); completion = nil }
    func remove(_ url: URL) async throws { try await writer.remove(url) }
}

private actor ControlledFileRead: HostTransport {
    private var delayFirstRead: Bool
    private var pending: CheckedContinuation<Data, any Error>?
    var readPending: Bool { pending != nil }

    init(delayFirstRead: Bool = false) { self.delayFirstRead = delayFirstRead }

    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        #expect(method == "readFile")
        if delayFirstRead {
            delayFirstRead = false
            return try await withCheckedThrowingContinuation { pending = $0 }
        }
        return Self.emptyChunk
    }

    func releaseRead() { pending?.resume(returning: Self.emptyChunk); pending = nil }
    private static var emptyChunk: Data { Data(#"{"data":"","size":0}"#.utf8) }
    nonisolated func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { AsyncThrowingStream { $0.finish() } }
    nonisolated func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }
}

private func emptyFile(_ id: String) -> SharedFile {
    SharedFile(id: id, name: "empty", size: 0,
               sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
}

private func fileEntry() -> Entry {
    Entry(id: "entry", seq: 1, botId: "bot", rev: 1, kind: "agent", turn: 1,
          data: EntryData(), createdAt: 0, updatedAt: 0)
}

@MainActor
private func downloadEventually(_ condition: @MainActor () async -> Bool) async -> Bool {
    for _ in 0..<200 {
        let ready = await condition()
        if ready { return true }
        try? await Task.sleep(for: .milliseconds(10))
    }
    return false
}

@MainActor @Test func cancellingCompletedFileDiscardsItAndAllowsAnotherExport() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let writer = ControlledDownloadWriter(directory: directory, pauseCompletedFile: true)
    let downloads = FileDownloads(writer: writer)
    defer { downloads.retire() }
    let client = HostClient(transport: ControlledFileRead())
    let entry = fileEntry()
    let first = emptyFile("cancelled")
    downloads.save(client: client, entry: entry, file: first)
    let completed = await downloadEventually {
        let url = await writer.pausedURL
        return url != nil
    }
    try #require(completed)
    let completedURL = await writer.pausedURL
    let discardedURL = try #require(completedURL)
    #expect(FileManager.default.fileExists(atPath: discardedURL.path))

    downloads.cancel(first.id)
    await writer.releaseCompletedFile()
    let discarded = await downloadEventually { !FileManager.default.fileExists(atPath: discardedURL.path) }
    try #require(discarded)
    #expect(downloads.states[first.id] == nil)
    #expect(downloads.export == nil)

    let next = emptyFile("next")
    // Retry through the public action while asynchronous cancellation settles.
    let exported = await downloadEventually {
        downloads.save(client: client, entry: entry, file: next)
        return downloads.export?.id == next.id
    }
    try #require(exported)
    let item = try #require(downloads.export)
    #expect(try Data(contentsOf: item.url).isEmpty)
    #expect(item.botId == entry.botId)
    downloads.dismissExport()
    let removed = await downloadEventually { !FileManager.default.fileExists(atPath: item.url.path) }
    #expect(removed)
}

@MainActor @Test func cancellingOutstandingFileReadAllowsRetryAndExport() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let writer = ControlledDownloadWriter(directory: directory, pauseCompletedFile: false)
    let downloads = FileDownloads(writer: writer)
    defer { downloads.retire() }
    let transport = ControlledFileRead(delayFirstRead: true)
    let client = HostClient(transport: transport)
    let entry = fileEntry()
    let file = emptyFile("retry")
    downloads.save(client: client, entry: entry, file: file)
    let pending = await downloadEventually {
        let pending = await transport.readPending
        return pending
    }
    try #require(pending)

    downloads.cancel(file.id)
    await transport.releaseRead()
    let cancelled = await downloadEventually {
        let folders = try? FileManager.default.contentsOfDirectory(atPath: directory.path)
        return folders?.isEmpty == true
    }
    try #require(cancelled)
    #expect(downloads.export == nil)
    #expect(downloads.states[file.id] == nil)

    let exported = await downloadEventually {
        downloads.save(client: client, entry: entry, file: file)
        return downloads.export?.id == file.id
    }
    try #require(exported)
    let item = try #require(downloads.export)
    #expect(try Data(contentsOf: item.url).isEmpty)
    downloads.dismissExport()
    let removed = await downloadEventually { !FileManager.default.fileExists(atPath: item.url.path) }
    #expect(removed)
}

@MainActor @Test func anUnshownExportIsReplacedByTheNextDownload() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let downloads = FileDownloads(writer: ControlledDownloadWriter(directory: directory, pauseCompletedFile: false))
    defer { downloads.retire() }
    let client = HostClient(transport: ControlledFileRead())
    let entry = fileEntry()
    // The first export's chat was left, so its sheet never showed or dismissed it.
    downloads.save(client: client, entry: entry, file: emptyFile("stale"))
    let staleReady = await downloadEventually { downloads.export?.id == "stale" }
    try #require(staleReady)
    let stale = try #require(downloads.export)

    downloads.save(client: client, entry: entry, file: emptyFile("next"))
    let replaced = await downloadEventually { downloads.export?.id == "next" }
    #expect(replaced)
    let staleRemoved = await downloadEventually { !FileManager.default.fileExists(atPath: stale.url.path) }
    #expect(staleRemoved)
}
