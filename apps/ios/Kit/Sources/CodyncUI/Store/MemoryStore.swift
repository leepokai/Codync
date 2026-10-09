import CodyncKit
import Foundation
import Observation

@MainActor @Observable
final class MemoryStore {
    let botId: String
    private let client: () async throws -> HostClient
    var facts: [MemoryFact] = []
    var query = ""
    var filter = "all"
    var total = 0
    var nextOffset: Int?
    var loaded = false
    var busy = false
    var error: String?
    var detail: MemoryDetail?
    var backup = ""
    private var request = 0
    private var detailRequest = 0
    private var searchTask: Task<Void, Never>?

    init(botId: String, client: @escaping () async throws -> HostClient) {
        self.botId = botId
        self.client = client
    }

    func load(more: Bool = false) async {
        request += 1
        let asked = request
        guard let offset = more ? nextOffset : 0 else { return }
        error = nil
        do {
            let listing = try await client().memory(botId: botId, query: query, filter: filter, offset: offset)
            guard asked == request else { return }
            facts = more ? facts + listing.facts : listing.facts
            total = listing.total
            nextOffset = listing.nextOffset
            loaded = true
        } catch {
            if asked == request { self.error = error.localizedDescription }
        }
    }

    func search(_ query: String, filter: String? = nil) {
        self.query = query
        if let filter { self.filter = filter }
        request += 1
        searchTask?.cancel()
        searchTask = Task {
            do { try await Task.sleep(for: .milliseconds(250)) } catch { return }
            await load()
        }
    }

    func action(_ method: String, fact: MemoryFact? = nil) async {
        _ = await perform {
            try await self.client().memoryAction(method, botId: self.botId, id: fact?.id, pinned: fact.map { !$0.pinned })
        }
    }

    func save(_ draft: MemoryDraft) async -> Bool {
        await perform { try await self.client().saveMemory(draft) }
    }

    func inspect(_ fact: MemoryFact, cursor: Int? = nil) async {
        detailRequest += 1
        let asked = detailRequest
        error = nil
        if cursor == nil { detail = nil }
        do {
            let value = try await client().memoryDetail(botId: botId, id: fact.id, cursor: cursor)
            if asked == detailRequest { detail = value }
        } catch {
            if asked == detailRequest { self.error = error.localizedDescription }
        }
    }

    func export() async {
        backup = ""
        error = nil
        do { backup = try await client().exportMemory(botId: botId) }
        catch { self.error = error.localizedDescription }
    }

    func importBackup(_ text: String) async -> Bool {
        await perform { try await self.client().importMemory(botId: self.botId, json: text) }
    }

    private func perform(_ work: () async throws -> Void) async -> Bool {
        guard !busy else { return false }
        busy = true
        error = nil
        defer { busy = false }
        do {
            try await work()
            await load()
            return true
        } catch {
            self.error = error.localizedDescription
            return false
        }
    }
}
