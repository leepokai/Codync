import Foundation
import Network
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "HookServer")

/// Claude Code session state derived from hook events (like Command app)
enum ClaudeState: String, Sendable {
    case working
    case waitingForUser
    case needsPermission
}

/// Per-session hook state
struct ClaudeSessionInfo: Sendable {
    var state: ClaudeState
    var lastEvent: String // e.g. "Using Bash", "Reading file", "Waiting for input"
    var toolName: String? // e.g. "Bash", "Read", "Edit", "Write"
}

/// Lightweight HTTP server that receives Claude Code hook events for real-time updates.
/// Tracks per-session Claude state + lastEvent from events (working/waiting/needsPermission).
/// Auto-configures hooks in ~/.claude/settings.json on launch.
final class ClaudeHookServer: @unchecked Sendable {
    private var listener: NWListener?
    private let port: UInt16
    private let queue = DispatchQueue(label: "com.pokai.CodePulse.hooks", qos: .utility)
    private let lock = NSLock()

    private var _sessions: [String: ClaudeSessionInfo] = [:]

    /// Thread-safe read of session info
    var sessions: [String: ClaudeSessionInfo] {
        lock.lock()
        defer { lock.unlock() }
        return _sessions
    }

    /// Convenience: just the states
    var sessionStates: [String: ClaudeState] {
        let s = sessions
        return s.mapValues { $0.state }
    }

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
        let events = ["PreToolUse", "Stop", "Notification", "SessionStart", "SessionEnd"]
        let codePulseHook: [String: Any] = ["type": "http", "url": hookURL]
        var changed = false

        // Clean up stale PostToolUse hooks if present (no longer needed)
        if var postEntries = hooks["PostToolUse"] as? [[String: Any]] {
            let before = postEntries.count
            postEntries.removeAll { entry in
                guard let entryHooks = entry["hooks"] as? [[String: Any]] else { return false }
                return entryHooks.contains { ($0["url"] as? String) == hookURL }
            }
            if postEntries.count != before {
                hooks["PostToolUse"] = postEntries.isEmpty ? nil : postEntries
                changed = true
            }
        }

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

    // MARK: - Event Processing

    private func processRequest(_ data: Data) {
        guard let text = String(data: data, encoding: .utf8),
              let bodyStart = text.range(of: "\r\n\r\n") else { return }

        let body = String(text[bodyStart.upperBound...])
        guard let bodyData = body.data(using: .utf8),
              let json = try? JSONSerialization.jsonObject(with: bodyData) as? [String: Any] else { return }

        let eventName = json["hook_event_name"] as? String ?? "unknown"
        let sessionId = json["session_id"] as? String

        guard let sessionId else {
            logger.debug("Hook: \(eventName) (no session_id)")
            DispatchQueue.main.async { [weak self] in self?.onEvent?() }
            return
        }

        let toolName = json["tool_name"] as? String

        switch eventName {
        case "SessionStart":
            updateSession(sessionId, state: .working, lastEvent: "Session started")

        case "PreToolUse":
            let displayTool = formatToolName(toolName)
            updateSession(sessionId, state: .working, lastEvent: "Using \(displayTool)", toolName: toolName)

        case "Stop":
            updateSession(sessionId, state: .waitingForUser, lastEvent: "Waiting for input")

        case "Notification":
            let notificationType = json["notification_type"] as? String
                ?? (json["message"] as? [String: Any])?["notification_type"] as? String

            switch notificationType {
            case "permission_prompt":
                updateSession(sessionId, state: .needsPermission, lastEvent: "Needs permission")
            case "idle_prompt":
                updateSession(sessionId, state: .waitingForUser, lastEvent: "Waiting for input")
            default:
                // Don't change state for unknown notifications
                logger.debug("Hook: Notification type=\(notificationType ?? "nil") for \(sessionId.prefix(8))...")
            }

        case "SessionEnd":
            lock.lock()
            _sessions.removeValue(forKey: sessionId)
            lock.unlock()
            logger.debug("Hook: SessionEnd → session \(sessionId.prefix(8))... removed")

        default:
            logger.debug("Hook: unknown event \(eventName)")
        }

        DispatchQueue.main.async { [weak self] in self?.onEvent?() }
    }

    private func updateSession(_ sessionId: String, state: ClaudeState, lastEvent: String, toolName: String? = nil) {
        lock.lock()
        _sessions[sessionId] = ClaudeSessionInfo(state: state, lastEvent: lastEvent, toolName: toolName)
        lock.unlock()
        logger.debug("Hook: \(lastEvent) → session \(sessionId.prefix(8))... → \(state.rawValue)")
    }

    private func formatToolName(_ name: String?) -> String {
        guard let name else { return "tool" }
        switch name {
        case "Bash": return "Bash"
        case "Read": return "Read"
        case "Edit": return "Edit"
        case "Write": return "Write"
        case "Glob": return "Glob"
        case "Grep": return "Grep"
        case "Agent": return "Agent"
        case "TodoWrite", "TaskCreate", "TaskUpdate": return "Tasks"
        case "WebFetch": return "WebFetch"
        case "WebSearch": return "WebSearch"
        default: return name
        }
    }
}
