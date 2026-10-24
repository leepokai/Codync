import Foundation

/// A long-form memory file: a Markdown note on one topic, as the host last recorded it.
public struct MemoryFile: Decodable, Identifiable, Hashable, Sendable {
    /// The bot whose folder holds it; nil for what all bots share.
    public var bot: String?
    public var name: String
    public var description: String?
    public var tokens: Int
    public var deleted: Bool
    public var updatedAt: Int64

    public var id: String { name }
}

/// One layer's folder of files.
public struct MemoryFolder: Decodable, Hashable, Sendable {
    public var files: [MemoryFile]
    /// Where it is on the computer.
    public var folder: String
}

/// Both layers' files, with the caps the host marks.
public struct MemoryFiles: Decodable, Hashable, Sendable {
    public var yours: MemoryFolder
    public var bot: MemoryFolder
    /// Tokens one file may hold.
    public var cap: Int
    /// Files one folder may hold.
    public var folderCap: Int
}

/// A file as it is on disk now.
public struct MemoryFileDetail: Decodable, Hashable, Sendable {
    public var file: String
    public var shared: Bool
    /// nil when the file is gone.
    public var content: String?
    public var path: String
}

extension HostClient {
    public func memoryFile(botId: String, file: String, shared: Bool) async throws -> MemoryFileDetail {
        struct Body: Encodable { var botId: String; var file: String; var shared: Bool }
        return try await call("memoryFile", Body(botId: botId, file: file, shared: shared))
    }
}
