import CodyncKit
import Foundation
import Observation

@MainActor @Observable
final class IconEditor {
    var busy = false
    var error: String?

    struct Change: Encodable {
        var target: String
        var data: String?
        var url: String?
        var tinted: Bool?
        var remove: Bool?
    }

    func save(_ change: Change, store: BotStore, file: URL? = nil) async {
        guard !busy else { return }
        busy = true
        error = nil
        defer { busy = false }
        do {
            guard let client = store.client, store.connection == .online else {
                throw HostError.unreachable
            }
            let epoch = store.iconsEpoch
            var change = change
            if let file { change.data = try await Self.read(file).base64EncodedString() }
            struct Reply: Decodable { var icons: [String: CustomIcon]; var iconsRevision: Int64 }
            let reply: Reply = try await client.call("setIcon", change, timeout: 60)
            guard epoch == store.iconsEpoch else { return }
            store.applyIcons(reply.icons, revision: reply.iconsRevision)
            store.scheduleSave()
        } catch { self.error = error.localizedDescription }
    }

    nonisolated private static func read(_ url: URL) async throws -> Data {
        try await Task.detached {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            let handle = try FileHandle(forReadingFrom: url)
            defer { try? handle.close() }
            let data = try handle.read(upToCount: 5 * 1024 * 1024 + 1) ?? Data()
            guard !data.isEmpty, data.count <= 5 * 1024 * 1024 else {
                throw NSError(domain: "CustomIcon", code: 1, userInfo: [NSLocalizedDescriptionKey: "Choose an image up to 5 MiB."])
            }
            return data
        }.value
    }
}
