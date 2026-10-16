import Foundation
import Network
import Testing
@testable import ScreenLink

@MainActor private func eventually(_ message: String, _ condition: () -> Bool) async throws {
    for _ in 0..<1000 {
        if condition() { return }
        try await Task.sleep(for: .milliseconds(5))
    }
    throw HelperError("socket check timed out: \(message)")
}

@MainActor private final class Host {
    let listener: NWListener
    private(set) var isReady = false
    private(set) var connections: [NWConnection] = []
    private(set) var replies: [ObjectIdentifier: [[String: Any]]] = [:]
    private var buffers: [ObjectIdentifier: Data] = [:]

    init() throws {
        listener = try NWListener(using: .tcp, on: .any)
        listener.stateUpdateHandler = { [weak self] state in
            MainActor.assumeIsolated {
                if case .ready = state { self?.isReady = true }
            }
        }
        listener.newConnectionHandler = { [weak self] c in
            MainActor.assumeIsolated {
                guard let self else { return }
                self.connections.append(c)
                c.start(queue: .main)
                self.receive(on: c)
            }
        }
        listener.start(queue: .main)
    }

    func send(_ method: String, on c: NWConnection) {
        let data = Data((#"{"id":1,"method":"\#(method)","params":{}}"# + "\n").utf8)
        c.send(content: data, completion: .contentProcessed { _ in })
    }

    private func receive(on c: NWConnection) {
        c.receive(minimumIncompleteLength: 1, maximumLength: 1 << 20) { [weak self] data, _, done, error in
            MainActor.assumeIsolated {
                guard let self else { return }
                let key = ObjectIdentifier(c)
                if let data {
                    self.buffers[key, default: Data()].append(data)
                    while let nl = self.buffers[key]?.firstIndex(of: 0x0a), let buffer = self.buffers[key] {
                        let line = buffer[buffer.startIndex..<nl]
                        self.buffers[key]?.removeSubrange(buffer.startIndex...nl)
                        if let reply = try? JSONSerialization.jsonObject(with: line) as? [String: Any] {
                            self.replies[key, default: []].append(reply)
                        }
                    }
                }
                if !done && error == nil { self.receive(on: c) }
            }
        }
    }

    func stop() {
        listener.cancel()
        for c in connections { c.cancel() }
    }
}

@Test(arguments: [false, true]) @MainActor func oldSocketRepliesCannotAnswerANewerConnectionsRequest(throwsError: Bool) async throws {
    let host = try Host()
    defer { host.stop() }
    try await eventually("listener ready") { host.isReady }
    let port = try #require(host.listener.port)
    let link = HostLink(endpoint: .hostPort(host: "127.0.0.1", port: port))
    defer { link.stop() }
    var pending: CheckedContinuation<Void, Never>?
    var resumed = false
    link.handler = { method, _ in
        if method == "old" {
            await withCheckedContinuation { pending = $0 }
            resumed = true
            if throwsError { throw HelperError("old socket failure") }
        }
        return ["method": method]
    }
    link.start()
    try await eventually("first connection ready") { host.connections.count == 1 && link.isConnected }
    host.send("old", on: host.connections[0])
    try await eventually("first handler suspended") { pending != nil }
    host.connections[0].cancel()
    try await eventually("replacement connection ready") { host.connections.count == 2 && link.isConnected }
    let replacement = host.connections[1]
    let key = ObjectIdentifier(replacement)
    host.send("new", on: replacement)
    try await eventually("replacement reply received") { host.replies[key]?.count == 1 }
    pending?.resume()
    pending = nil
    try await eventually("first handler resumed") { resumed }
    try await Task.sleep(for: .milliseconds(100))
    let replies = host.replies[key, default: []]
    #expect(replies.count == 1, "the old handler must not reuse its request ID on the new socket")
    #expect((replies.first?["result"] as? [String: String])?["method"] == "new")
}
