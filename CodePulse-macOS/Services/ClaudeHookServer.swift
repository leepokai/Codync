import Foundation
import Network
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "HookServer")

/// Lightweight HTTP server that receives Claude Code hook events for real-time updates.
/// Auto-configures hooks in ~/.claude/settings.json on launch.
final class ClaudeHookServer: @unchecked Sendable {
    private var listener: NWListener?
    private let port: UInt16
    private let queue = DispatchQueue(label: "com.pokai.CodePulse.hooks", qos: .utility)
    var onEvent: (@Sendable () -> Void)?

    init(port: UInt16 = 19221) {
        self.port = port
    }

    // MARK: - Auto-configure hooks

    func ensureHooksConfigured() {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let settingsURL = home.appendingPathComponent(".claude/settings.json")
        let hookURL = "http://localhost:\(port)/codepulse-event"

        var root: [String: Any]
        if FileManager.default.fileExists(atPath: settingsURL.path) {
            guard let data = try? Data(contentsOf: settingsURL),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                logger.warning("Could not parse ~/.claude/settings.json, skipping hook config")
                return
            }
            root = json
        } else {
            let claudeDir = home.appendingPathComponent(".claude")
            try? FileManager.default.createDirectory(at: claudeDir, withIntermediateDirectories: true)
            root = [:]
        }

        var hooks = root["hooks"] as? [String: Any] ?? [:]
        let events = ["PostToolUse", "Stop", "SessionStart", "SessionEnd"]
        let codePulseHook: [String: Any] = ["type": "http", "url": hookURL]
        var changed = false

        for event in events {
            var entries = hooks[event] as? [[String: Any]] ?? []
            let alreadyPresent = entries.contains { entry in
                guard let entryHooks = entry["hooks"] as? [[String: Any]] else { return false }
                return entryHooks.contains { ($0["url"] as? String) == hookURL }
            }
            if !alreadyPresent {
                entries.append(["hooks": [codePulseHook]])
                hooks[event] = entries
                changed = true
            }
        }

        if changed {
            root["hooks"] = hooks
            if let data = try? JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys]) {
                try? data.write(to: settingsURL, options: .atomic)
                logger.info("Auto-configured hooks in ~/.claude/settings.json")
            }
        } else {
            logger.debug("Hooks already configured")
        }
    }

    // MARK: - Server

    func start() {
        do {
            let params = NWParameters.tcp
            params.allowLocalEndpointReuse = true
            params.requiredLocalEndpoint = NWEndpoint.hostPort(
                host: .init("127.0.0.1"),
                port: NWEndpoint.Port(rawValue: port)!
            )
            listener = try NWListener(using: params)
        } catch {
            logger.error("Failed to create listener: \(error.localizedDescription)")
            return
        }

        listener?.stateUpdateHandler = { state in
            switch state {
            case .ready:
                logger.info("Hook server listening on port \(self.port)")
            case .failed(let error):
                logger.error("Hook server failed: \(error.localizedDescription)")
            default:
                break
            }
        }

        listener?.newConnectionHandler = { [weak self] connection in
            self?.handleConnection(connection)
        }

        listener?.start(queue: queue)
    }

    func stop() {
        listener?.cancel()
        listener = nil
        logger.info("Hook server stopped")
    }

    // MARK: - Connection

    private func handleConnection(_ connection: NWConnection) {
        connection.start(queue: queue)

        // Use a class wrapper to avoid capturing mutable var in Sendable closure
        final class DataAccumulator: @unchecked Sendable {
            var data = Data()
        }
        let accumulator = DataAccumulator()

        func receiveMore() {
            connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
                if let data { accumulator.data.append(data) }

                if accumulator.data.count > 512_000 {
                    connection.cancel()
                    return
                }

                let done = isComplete || error != nil
                let hasFullRequest: Bool = {
                    guard let text = String(data: accumulator.data, encoding: .utf8) else { return false }
                    return text.contains("\r\n\r\n")
                }()

                if done || hasFullRequest {
                    self?.processRequest(accumulator.data)
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}"
                    connection.send(content: response.data(using: .utf8), completion: .contentProcessed { _ in
                        connection.cancel()
                    })
                } else {
                    receiveMore()
                }
            }
        }

        receiveMore()
    }

    private func processRequest(_ data: Data) {
        guard let text = String(data: data, encoding: .utf8) else { return }

        var eventName = "unknown"
        if let bodyStart = text.range(of: "\r\n\r\n") {
            let body = String(text[bodyStart.upperBound...])
            if let bodyData = body.data(using: .utf8),
               let json = try? JSONSerialization.jsonObject(with: bodyData) as? [String: Any] {
                eventName = json["hook_event_name"] as? String ?? "unknown"
            }
        }

        logger.debug("Hook event: \(eventName)")

        // Trigger immediate rescan on main thread
        DispatchQueue.main.async { [weak self] in
            self?.onEvent?()
        }
    }
}
