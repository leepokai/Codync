import Foundation

/// One thing in a chat's Library (`host/src/chat/library.rs`): an artifact its bot published, a file the bot shared, or
/// one the user attached, with when (an artifact's last publish, a file's message).
public struct ChatLibraryItem: Hashable, Sendable, Identifiable {
    public enum Kind: Hashable, Sendable {
        case artifact(Artifact)
        /// Saved like its chat card: `readFile` with the message's `entryId`.
        case file(SharedFile, entryId: String)
        /// Opened like in the chat: `readUpload`.
        case attachment(Attachment, entryId: String)
    }

    public var kind: Kind
    public var at: Int64

    public init(_ kind: Kind, at: Int64) {
        self.kind = kind
        self.at = at
    }

    public init(_ artifact: Artifact) { self.init(.artifact(artifact), at: artifact.updatedAt) }

    public var id: String {
        switch kind {
        case .artifact(let artifact): "artifact/\(artifact.id)"
        case .file(let file, let entryId): "file/\(entryId)/\(file.id)"
        case .attachment(let file, let entryId): "attachment/\(entryId)/\(file.id)"
        }
    }

    public var date: Date { Date(milliseconds: at) }

    /// What `library` sends; a kind or shape this app can't read becomes nil.
    struct Wire: Decodable {
        struct File: Decodable {
            var id: String
            var name: String
            var size: Int64
            var sha256: String?
        }

        var kind: String
        var at: Int64
        var entryId: String?
        var artifact: Artifact?
        var file: File?

        private enum CodingKeys: String, CodingKey { case kind, at, entryId, artifact, file }

        init(from decoder: any Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            kind = (try? c.decode(String.self, forKey: .kind)) ?? ""
            at = (try? c.decode(Int64.self, forKey: .at)) ?? 0
            entryId = try? c.decode(String.self, forKey: .entryId)
            artifact = try? c.decode(Artifact.self, forKey: .artifact)
            file = try? c.decode(File.self, forKey: .file)
        }

        var item: ChatLibraryItem? {
            switch (kind, artifact, file, entryId) {
            case ("artifact", let artifact?, _, _):
                ChatLibraryItem(.artifact(artifact), at: at)
            case ("file", _, let file?, let entryId?):
                file.sha256.map {
                    ChatLibraryItem(.file(SharedFile(id: file.id, name: file.name, size: file.size, sha256: $0), entryId: entryId), at: at)
                }
            case ("attachment", _, let file?, let entryId?):
                ChatLibraryItem(.attachment(Attachment(id: file.id, name: file.name, size: file.size), entryId: entryId), at: at)
            default:
                nil
            }
        }
    }
}
