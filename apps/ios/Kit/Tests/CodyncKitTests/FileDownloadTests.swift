import CryptoKit
import Foundation
import Testing
@testable import CodyncKit

private struct FileTransport: HostTransport {
    let bytes: Data
    var truncate = false
    var wrongSize = false
    func call(_ method: String, body: Data, timeout: TimeInterval) async throws -> Data {
        #expect(method == "readFile")
        let request = try #require(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let offset = try #require(request["offset"] as? Int)
        let end = min(offset + FileDownload.chunkSize, bytes.count)
        let data = truncate ? Data() : bytes.subdata(in: offset..<end)
        return try JSONSerialization.data(withJSONObject: ["data": data.base64EncodedString(), "size": bytes.count + (wrongSize ? 1 : 0)])
    }
    func stream(_ request: HostStreamRequest) -> AsyncThrowingStream<Data, Error> { AsyncThrowingStream { $0.finish() } }
    func states() -> AsyncStream<LinkState> { AsyncStream { $0.finish() } }
}

private func metadata(_ data: Data, name: String = "file.bin") -> SharedFile {
    SharedFile(id: UUID().uuidString, name: name, size: Int64(data.count), sha256: SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined())
}

@Test func generatedFilesDownloadExactBinaryAndEmptyBytes() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let writer = FileDownload()
    for bytes in [Data(), Data((0..<(FileDownload.chunkSize + 7)).map { UInt8($0 % 256) })] {
        let file = metadata(bytes, name: ".download-partial")
        let url = try await writer.save(client: HostClient(transport: FileTransport(bytes: bytes)), entryId: "entry", file: file, directory: directory) { _ in }
        #expect(try Data(contentsOf: url) == bytes)
        #expect(url.lastPathComponent == ".download-partial")
        try await writer.remove(url)
    }
}

@Test func generatedFileFailuresRemovePartialBytes() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let bytes = Data([0, 255, 42])
    let writer = FileDownload()
    for transport in [FileTransport(bytes: bytes, truncate: true), FileTransport(bytes: bytes, wrongSize: true)] {
        await #expect(throws: FileDownloadError.self) {
            try await writer.save(client: HostClient(transport: transport), entryId: "entry", file: metadata(bytes), directory: directory) { _ in }
        }
        #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
    }
    let bad = SharedFile(id: "id", name: "file.bin", size: Int64(bytes.count), sha256: String(repeating: "0", count: 64))
    await #expect(throws: FileDownloadError.self) {
        try await writer.save(client: HostClient(transport: FileTransport(bytes: bytes)), entryId: "entry", file: bad, directory: directory) { _ in }
    }
    #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
}

@Test func generatedFileCancellationRemovesPartialBytes() async throws {
    let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    defer { try? FileManager.default.removeItem(at: directory) }
    let bytes = Data(repeating: 42, count: FileDownload.chunkSize + 1)
    let writer = FileDownload()
    let task = Task {
        try await writer.save(client: HostClient(transport: FileTransport(bytes: bytes)), entryId: "entry", file: metadata(bytes), directory: directory) { _ in
            withUnsafeCurrentTask { $0?.cancel() }
        }
    }
    await #expect(throws: CancellationError.self) { try await task.value }
    #expect(try FileManager.default.contentsOfDirectory(atPath: directory.path).isEmpty)
}

@Test func generatedFileMetadataIsAdditiveAndRejectsUnsafeNames() throws {
    let old = try JSONDecoder().decode(EntryData.self, from: Data(#"{"text":"hello","final":true}"#.utf8))
    #expect(old.files == nil)
    for name in ["../escape", "bad\\file", "..", "\n"] {
        #expect(throws: FileDownloadError.self) { try metadata(Data(), name: name).validate() }
    }
    try metadata(Data(), name: ".hidden").validate()
}
