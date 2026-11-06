import Foundation
import os

private let log = Logger(subsystem: "com.pokai.Codync", category: "Call")

/// A provider's WebSocket, text frames each way (Gemini Live); tests replay a recorded one.
@MainActor
protocol LiveSocket: AnyObject {
    func send(_ text: String)
    /// The next frame; throws once the socket closed.
    func receive() async throws -> String
    func close()
    /// The code the server closed it with (0 when it didn't say).
    var closeCode: Int { get }
    /// Why the server closed it, if it said.
    var closeReason: String { get }
}

final class URLSessionLiveSocket: LiveSocket {
    private let task: URLSessionWebSocketTask

    init(_ url: URL) {
        task = URLSession.shared.webSocketTask(with: url)
        task.maximumMessageSize = 16 << 20
        task.resume()
    }

    func send(_ text: String) {
        task.send(.string(text)) { error in
            if let error { log.error("socket send: \(error.localizedDescription, privacy: .public)") }
        }
    }

    func receive() async throws -> String {
        switch try await task.receive() {
        case .data(let data): String(decoding: data, as: UTF8.self)
        case .string(let text): text
        @unknown default: ""
        }
    }

    func close() { task.cancel(with: .normalClosure, reason: nil) }

    var closeCode: Int { task.closeCode.rawValue }

    var closeReason: String { task.closeReason.map { String(decoding: $0, as: UTF8.self) } ?? "" }
}
