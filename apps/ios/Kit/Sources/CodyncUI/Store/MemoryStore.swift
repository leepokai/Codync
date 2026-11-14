import CodyncKit
import Foundation
import Observation

/// One bot's memory, read-only: the bot changes it.
@MainActor @Observable
final class MemoryStore {
    let botId: String
    private let client: () async throws -> HostClient
    var view: MemoryView?
    var error: String?
    private var request = 0

    init(botId: String, client: @escaping () async throws -> HostClient) {
        self.botId = botId
        self.client = client
    }

    func load() async {
        request += 1
        let asked = request
        do {
            let value = try await client().memory(botId: botId)
            if asked == request {
                view = value
                error = nil
            }
        } catch {
            // A host before 2.18.0 answers with its old memory listing, which has no layers or dreams.
            if asked == request {
                self.error = error is DecodingError ? "Update Codync on your computer to see this bot's memory." : error.localizedDescription
            }
        }
    }

    /// A long-form file as it is now.
    func file(_ name: String, shared: Bool) async -> MemoryFileDetail? {
        do {
            return try await client().memoryFile(botId: botId, file: name, shared: shared)
        } catch {
            self.error = error.localizedDescription
            return nil
        }
    }
}
