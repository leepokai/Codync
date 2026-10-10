import CodyncKit
import Foundation
import Observation

protocol FileDownloadWriting: Sendable {
    func save(client: HostClient, entryId: String, file: SharedFile,
              directory: URL, progress: @Sendable (Int64) async -> Void) async throws -> URL
    func remove(_ url: URL) async throws
}

extension FileDownload: FileDownloadWriting {}

@MainActor @Observable
public final class FileDownloads {
    public enum State {
        case downloading(Int64), saved, failed(String)
    }
    public struct Export: Identifiable {
        public let id: String
        public let url: URL
        public let botId: String
        public let threadId: String?
    }
    public private(set) var states: [String: State] = [:]
    public var export: Export?
    @ObservationIgnored private var exported: Export?
    @ObservationIgnored private var tasks: [String: Task<Void, Never>] = [:]
    @ObservationIgnored private let writer: any FileDownloadWriting
    @ObservationIgnored private var retired = false

    public init() { writer = FileDownload() }

    init(writer: any FileDownloadWriting) { self.writer = writer }

    public func save(client: HostClient?, entry: Entry, file: SharedFile) {
        guard !retired else { return }
        guard tasks.isEmpty else {
            if tasks[file.id] == nil { states[file.id] = .failed("Finish the current download first.") }
            return
        }
        // This file's export sheet is already waiting in its chat.
        if export?.id == file.id { return }
        // Another file's export whose chat was left never showed its sheet; this tap replaces it.
        dismissExport()
        guard let client else { states[file.id] = .failed("Connect to this computer to download the file."); return }
        states[file.id] = .downloading(0)
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("CodyncDownloads", isDirectory: true)
        tasks[file.id] = Task {
            defer { tasks[file.id] = nil }
            do {
                let url = try await writer.save(client: client, entryId: entry.id, file: file, directory: directory) { [weak self] received in
                    await self?.update(file.id, received: received)
                }
                guard !Task.isCancelled, !retired else { try await writer.remove(url); return }
                states[file.id] = .saved
                let item = Export(id: file.id, url: url, botId: entry.botId, threadId: entry.threadId)
                exported = item
                export = item
            } catch is CancellationError {
                states[file.id] = nil
            } catch {
                if !Task.isCancelled, !retired { states[file.id] = .failed(error.localizedDescription) }
            }
        }
    }

    private func update(_ id: String, received: Int64) {
        guard !retired, tasks[id]?.isCancelled == false else { return }
        states[id] = .downloading(received)
    }

    public func cancel(_ id: String) { tasks[id]?.cancel(); states[id] = nil }

    /// The native share sheet has finished using the verified temporary file.
    public func dismissExport() {
        export = nil
        guard let item = exported else { return }
        exported = nil
        Task { do { try await writer.remove(item.url) } catch { states[item.id] = .failed(error.localizedDescription) } }
    }

    public func retire() {
        retired = true
        for task in tasks.values { task.cancel() }
        dismissExport()
        states.removeAll()
    }
}
