import CryptoKit
import Foundation

public enum FileDownloadError: LocalizedError {
    case invalidMetadata, invalidChunk, integrity
    public var errorDescription: String? {
        switch self {
        case .invalidMetadata: "Invalid file metadata."
        case .invalidChunk: "The download was truncated or changed. Retry the download."
        case .integrity: "The file integrity check failed. Retry the download."
        }
    }
}

public extension HostClient {
    func readFile(entryId: String, fileId: String, offset: Int64) async throws -> FileChunk {
        struct Body: Encodable { let entryId: String; let fileId: String; let offset: Int64 }
        return try await call("readFile", Body(entryId: entryId, fileId: fileId, offset: offset))
    }
}

public struct FileChunk: Decodable, Sendable {
    public let data: String
    public let size: Int64
}

/// Filesystem work and hashing stay off the main actor, with one chunk in memory.
public actor FileDownload {
    public init() {}
    public static let chunkSize = 384 * 1024

    public func save(client: HostClient, entryId: String, file: SharedFile,
                     directory: URL, progress: @Sendable (Int64) async -> Void) async throws -> URL {
        try file.validate()
        try Task.checkCancellation()
        let folder = directory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        var successful = false
        defer { if !successful { try? FileManager.default.removeItem(at: folder) } }
        let partial = folder.appendingPathComponent(".download-partial")
        guard FileManager.default.createFile(atPath: partial.path, contents: nil) else { throw CocoaError(.fileWriteUnknown) }
        let handle = try FileHandle(forWritingTo: partial)
        defer { try? handle.close() }
        try await read(client: client, entryId: entryId, file: file) { bytes, offset in
            try handle.write(contentsOf: bytes)
            await progress(offset)
        }
        try Task.checkCancellation()
        try handle.synchronize()
        try handle.close()
        // Keep a source named .download-partial valid too: the export lives in its own directory.
        let ready = folder.appendingPathComponent("ready", isDirectory: true)
        try FileManager.default.createDirectory(at: ready, withIntermediateDirectories: true)
        let destination = ready.appendingPathComponent(file.name)
        try FileManager.default.moveItem(at: partial, to: destination)
        successful = true
        return destination
    }

    /// The whole file in memory, checked like a saved one (pictures shown in the chat).
    public func data(client: HostClient, entryId: String, file: SharedFile) async throws -> Data {
        try file.validate()
        var data = Data(capacity: Int(file.size))
        try await read(client: client, entryId: entryId, file: file) { bytes, _ in data.append(bytes) }
        return data
    }

    /// Hands each chunk of `file` to `each` (with the bytes read so far); throws unless they add up to its size and digest.
    private func read(client: HostClient, entryId: String, file: SharedFile,
                      each: (Data, Int64) async throws -> Void) async throws {
        var offset: Int64 = 0
        var hash = SHA256()
        repeat {
            try Task.checkCancellation()
            let chunk = try await client.readFile(entryId: entryId, fileId: file.id, offset: offset)
            try Task.checkCancellation()
            guard chunk.size == file.size, chunk.data.utf8.count <= Self.chunkSize / 3 * 4,
                  let bytes = Data(base64Encoded: chunk.data),
                  bytes.count == min(Self.chunkSize, Int(file.size - offset)) else { throw FileDownloadError.invalidChunk }
            hash.update(data: bytes)
            offset += Int64(bytes.count)
            try await each(bytes, offset)
        } while offset < file.size
        let digest = hash.finalize().map { String(format: "%02x", $0) }.joined()
        guard offset == file.size, digest == file.sha256 else { throw FileDownloadError.integrity }
    }

    public func remove(_ url: URL) throws {
        try FileManager.default.removeItem(at: url.deletingLastPathComponent().deletingLastPathComponent())
    }
}
