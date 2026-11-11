import Foundation

/// A caller-owned, bounded stream of independently playable 24 kHz WAV frames.
public actor VoiceSpeechStream {
    private let client: HostClient
    private let requestId: String
    private var closed = false
    private struct Reply: Decodable { var done: Bool; var audio: String?; var format: String? }
    private struct Cancelled: Decodable { var cancelled: Bool }
    private struct Started: Decodable { var streaming: Bool }

    public init(client: HostClient) { self.client = client; requestId = UUID().uuidString }

    public func start(text: String, botId: String) async throws {
        let client = client, id = requestId
        try await withTaskCancellationHandler {
            try Task.checkCancellation()
            let _: Started = try await client.call("voiceStartSpeech", ["requestId":id,"text":text,"botId":botId], timeout:90)
            try Task.checkCancellation()
        } onCancel: { Task { let _: Cancelled? = try? await client.call("voiceCancelSpeech", ["requestId":id], timeout:10) } }
    }
    public func next() async throws -> (Data, String)? {
        guard !closed else { return nil }
        let client = client, id = requestId
        return try await withTaskCancellationHandler {
            try Task.checkCancellation()
            let reply: Reply = try await client.call("voiceSpeechChunk", ["requestId":id], timeout:90)
            try Task.checkCancellation()
            if reply.done { closed = true; return nil }
            guard let audio = reply.audio, reply.format == "wav", let data = Data(base64Encoded:audio) else {
                throw HostError.http(0,"Unreadable local speech frame.")
            }
            return (data,"wav")
        } onCancel: { Task { let _: Cancelled? = try? await client.call("voiceCancelSpeech", ["requestId":id], timeout:10) } }
    }
    public func cancel() async {
        closed = true
        let _: Cancelled? = try? await client.call("voiceCancelSpeech", ["requestId":requestId], timeout:10)
    }
}
