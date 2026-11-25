import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "BotStore")

/// The ended threads older than the bots' `work` carries, read a page at a time for the Threads list
/// (`olderThreads`), on a computer that keeps them (`hello.threadKeepDays`).
public extension BotStore {
    /// This computer lists its older threads now.
    var listsOlderThreads: Bool { hello?.threadKeepDays != nil && connection == .online }

    /// The first page, unless the list has one.
    func loadFirstOlderThreads() {
        if olderThreads.rows.isEmpty { loadOlderThreads() }
    }

    /// The next page while there is one, one request at a time.
    func loadOlderThreads() {
        guard listsOlderThreads, olderThreads.more, !loadingOlderThreads, let client else { return }
        let (generation, before) = (olderThreadsGeneration, olderThreads.cursor)
        loadingOlderThreads = true
        Task {
            defer { if generation == olderThreadsGeneration { loadingOlderThreads = false } }
            do {
                let page = try await client.olderThreads(before: before)
                guard generation == olderThreadsGeneration else { return }
                Motion.animate { olderThreads.add(page) }
            } catch {
                // The list's end asks again when it shows next.
                log.error("older threads: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    /// Drops what was paged: the Threads list asks for the first page again.
    internal func resetOlderThreads() {
        olderThreadsGeneration += 1
        olderThreads = OlderThreadList()
        loadingOlderThreads = false
    }
}
