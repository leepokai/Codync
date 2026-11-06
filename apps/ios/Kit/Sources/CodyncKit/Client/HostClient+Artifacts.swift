import CryptoKit
import Foundation

public struct ArtifactChunk: Decodable, Sendable {
    public let data: String
    public let size: Int64
    public let artifact: Artifact
}

public extension HostClient {
    /// The chat's Library, newest first. Kinds this app doesn't know are left out.
    func library(botId: String) async throws -> [ChatLibraryItem] {
        struct Res: Decodable { var items: [ChatLibraryItem.Wire] }
        let res: Res = try await call("library", ["botId": botId])
        return res.items.compactMap(\.item)
    }

    /// At most `FileDownload.chunkSize` bytes of an artifact's HTML, from `offset`.
    func readArtifact(botId: String, artifactId: String, offset: Int64) async throws -> ArtifactChunk {
        struct Body: Encodable { let botId: String; let artifactId: String; let offset: Int64 }
        return try await call("readArtifact", Body(botId: botId, artifactId: artifactId, offset: offset))
    }

    func renameArtifact(botId: String, artifactId: String, title: String) async throws -> Artifact {
        struct Res: Decodable { var artifact: Artifact }
        let res: Res = try await call("renameArtifact", ["botId": botId, "artifactId": artifactId, "title": title])
        return res.artifact
    }

    /// An artifact's whole HTML, chunk by chunk. Published again meanwhile, it reads the new version from its start.
    func readArtifactHTML(botId: String, artifactId: String) async throws -> (artifact: Artifact, html: Data) {
        var html = Data()
        var version: String?
        while true {
            let offset = html.count
            let chunk = try await readArtifact(botId: botId, artifactId: artifactId, offset: Int64(offset))
            if chunk.artifact.sha256 != version {
                version = chunk.artifact.sha256
                html = Data()
                if offset > 0 { continue }
            }
            guard let bytes = Data(base64Encoded: chunk.data), !bytes.isEmpty, Int64(html.count + bytes.count) <= chunk.size
            else { throw FileDownloadError.invalidChunk }
            html.append(bytes)
            guard Int64(html.count) == chunk.size else { continue }
            let digest = SHA256.hash(data: html).map { String(format: "%02x", $0) }.joined()
            guard digest == chunk.artifact.sha256 else { throw FileDownloadError.integrity }
            return (chunk.artifact, html)
        }
    }
}
