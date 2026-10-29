import CodyncKit
import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Artifacts")

extension BotStore {
    /// Whether the computer's host has the Library; an older one has no `library` method, so the app leaves it out.
    var hasLibrary: Bool { hostVersion.map { !AppVersion.isBelow($0.version, Artifact.firstHost) } ?? false }

    /// The chat's Library as the computer lists it now: its artifacts go to `artifacts` (where their events land too),
    /// the files shared in the chat come back.
    func loadLibrary(_ botId: String) async throws -> [ChatLibraryItem] {
        let items = try await withLink(replay: true) { try await $0.library(botId: botId) }
        return items.filter { item in
            guard case .artifact(let artifact) = item.kind else { return true }
            artifacts.upsert(artifact)
            return false
        }
    }

    /// A card on screen: the version it names is known now (history and the cache don't come as events), and the
    /// artifact's HTML is read unless the newest version is here.
    func showArtifact(_ ref: ArtifactRef, botId: String) {
        artifacts.announce(ref)
        loadArtifact(ref.id, botId: botId)
    }

    /// Reads an artifact's HTML in the background unless the newest version known is already here.
    func loadArtifact(_ id: String, botId: String) {
        guard !retired, artifacts.needsRead(id) else { return }
        artifacts.reading.insert(id)
        artifacts.failures[id] = nil
        Task {
            do {
                try await readArtifact(id, botId: botId)
                artifacts.reading.remove(id)
                // A version published while this one was read is read next.
                loadArtifact(id, botId: botId)
            } catch {
                artifacts.reading.remove(id)
                artifacts.failures[id] = error.localizedDescription
            }
        }
    }

    private func readArtifact(_ id: String, botId: String) async throws {
        let read = try await withLink(replay: true) { try await $0.readArtifactHTML(botId: botId, artifactId: id) }
        artifacts.store(read.artifact, html: String(decoding: read.html, as: UTF8.self))
    }

    func renameArtifact(_ artifact: Artifact, title: String) async throws {
        let renamed = try await withLink {
            try await $0.renameArtifact(botId: artifact.botId, artifactId: artifact.id, title: title)
        }
        artifacts.upsert(renamed)
    }

    /// The artifact's newest HTML as `<title>.html`, in a folder of its own for the share sheet
    /// (`removeArtifactFile`).
    func artifactFile(_ id: String, botId: String) async throws -> URL {
        if artifacts.html[id] == nil || artifacts.needsRead(id) { try await readArtifact(id, botId: botId) }
        guard let artifact = artifacts.details[id], let html = artifacts.html[id] else { throw FileDownloadError.invalidMetadata }
        let folder = FileManager.default.temporaryDirectory.appending(path: "CodyncArtifacts/\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let name = artifact.title.components(separatedBy: CharacterSet(charactersIn: "/:\\")).joined(separator: "-")
        let url = folder.appending(path: "\(name).html")
        try Data(html.text.utf8).write(to: url)
        return url
    }

    func removeArtifactFile(_ url: URL) {
        do {
            try FileManager.default.removeItem(at: url.deletingLastPathComponent())
        } catch {
            // The system clears the temporary folder in time anyway.
            log.debug("artifact file left behind: \(error.localizedDescription)")
        }
    }
}
