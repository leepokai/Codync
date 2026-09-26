import Foundation
import Network

struct HelperError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}

/// JSON-RPC 2.0, one message per line, over the host's `~/.codync/screen.sock`.
/// The host sends requests; we answer them and send notifications. Reconnects forever.
@MainActor
final class HostLink {
    typealias Handler = @MainActor (_ method: String, _ params: [String: Any]) async throws -> Any

    /// Answers the host's requests.
    var handler: Handler?
    private var connection: NWConnection?
    private var buffer = Data()
    private(set) var isConnected = false
    /// Runs on every (re)connect: the host forgets us when we drop.
    var onConnect: (@MainActor () -> Void)?

    static var socketPath: String {
        let base = ProcessInfo.processInfo.environment["CODYNC_HOME"] ?? NSHomeDirectory() + "/.codync"
        return base + "/screen.sock"
    }

    func start() {
        let c = NWConnection(to: .unix(path: Self.socketPath), using: .tcp)
        connection = c
        c.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated { self?.changed(state, of: c) }
        }
        c.start(queue: .main)
    }

    private func changed(_ state: NWConnection.State, of c: NWConnection) {
        guard c === connection else { return }
        switch state {
        case .ready:
            isConnected = true
            buffer = Data()
            receive(on: c)
            onConnect?()
        case .waiting, .failed:
            // The host isn't running (yet): try again shortly.
            drop(c)
        default:
            break
        }
    }

    private func drop(_ c: NWConnection) {
        guard c === connection else { return }
        isConnected = false
        connection = nil
        c.stateUpdateHandler = nil
        c.cancel()
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(2))
            self?.start()
        }
    }

    private func receive(on c: NWConnection) {
        c.receive(minimumIncompleteLength: 1, maximumLength: 1 << 20) { [weak self] data, _, done, error in
            MainActor.assumeIsolated {
                guard let self, c === self.connection else { return }
                if let data {
                    self.buffer.append(data)
                    self.drain()
                }
                if done || error != nil {
                    self.drop(c)
                } else {
                    self.receive(on: c)
                }
            }
        }
    }

    private func drain() {
        while let nl = buffer.firstIndex(of: 0x0A) {
            let line = buffer[buffer.startIndex..<nl]
            buffer.removeSubrange(buffer.startIndex...nl)
            guard let msg = (try? JSONSerialization.jsonObject(with: line)) as? [String: Any],
                  let method = msg["method"] as? String
            else { continue }
            let id = msg["id"].map { "\($0)" }
            let params = msg["params"] as? [String: Any] ?? [:]
            Task { await self.run(method, params, id: id) }
        }
    }

    private func run(_ method: String, _ params: [String: Any], id: String?) async {
        do {
            guard let handler else { throw HelperError("not ready") }
            let result = try await handler(method, params)
            if let id { send(["jsonrpc": "2.0", "id": Int(id) ?? 0, "result": result]) }
        } catch {
            if let id {
                send(["jsonrpc": "2.0", "id": Int(id) ?? 0, "error": ["code": -32000, "message": error.localizedDescription]])
            }
        }
    }

    func notify(_ method: String, _ params: [String: Any]) {
        send(["jsonrpc": "2.0", "method": method, "params": params])
    }

    private func send(_ msg: [String: Any]) {
        guard let connection, isConnected, var data = try? JSONSerialization.data(withJSONObject: msg) else { return }
        data.append(0x0A)
        connection.send(content: data, completion: .contentProcessed { _ in })
    }
}
