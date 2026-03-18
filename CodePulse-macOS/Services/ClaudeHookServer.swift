import Foundation
import Network
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "HookServer")

/// Hook-driven status detection server.
/// Receives events from 7 Claude Code hooks via notify.sh command script.
/// Hook events are the primary status source — JSONL parsing is supplementary.
final class ClaudeHookServer: @unchecked Sendable {
    private var listener: NWListener?
    private let port: UInt16
    private let queue = DispatchQueue(label: "com.pokai.CodePulse.hooks", qos: .utility)
    private let lock = NSLock()

    private var _permissionSessions: Set<String> = []
    private var _stopSessions: Set<String> = []
    private var _toolRunningSessions: Set<String> = []

    /// Maps cwd → most recently seen sessionId from hook events.
    /// This is the authoritative source for "what sessionId is active for this project",
    /// because hook events contain the REAL session_id (not the stale one in session files).
    private var _cwdToSessionId: [String: String] = [:]

    // MARK: - Thread-safe accessors

    func needsPermission(_ sessionId: String) -> Bool {
        withLock { _permissionSessions.contains(sessionId) }
    }

    func isStopped(_ sessionId: String) -> Bool {
        withLock { _stopSessions.contains(sessionId) }
    }

    func isToolRunning(_ sessionId: String) -> Bool {
        withLock { _toolRunningSessions.contains(sessionId) }
    }

    func clearPermission(_ sessionId: String) {
        withLock { _permissionSessions.remove(sessionId) }
    }

    func clearStop(_ sessionId: String) {
        withLock { _stopSessions.remove(sessionId) }
    }

    /// Remove all state for sessions no longer alive.
    func pruneStale(activeSessionIds: Set<String>) {
        withLock {
            _permissionSessions.formIntersection(activeSessionIds)
            _stopSessions.formIntersection(activeSessionIds)
            _toolRunningSessions.formIntersection(activeSessionIds)
        }
    }

    /// Get the real sessionId for a cwd, as reported by hook events.
    /// Returns nil if no hook event has been received for this cwd yet.
    func activeSessionId(forCwd cwd: String) -> String? {
        withLock { _cwdToSessionId[cwd] }
    }

    // MARK: - Callbacks

    var onHookSignal: (@Sendable (_ sessionId: String, _ signal: HookSignalType, _ toolName: String?) -> Void)?
    var onSessionEvent: (@Sendable () -> Void)?

    init(port: UInt16 = 19221) {
        self.port = port
    }

    // MARK: - Lock helper

    @discardableResult
    private func withLock<T>(_ body: () -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }

    /// Update all three state sets atomically for a session.
    private func setSessionState(
        _ sessionId: String,
        permission: Bool? = nil,
        stopped: Bool? = nil,
        toolRunning: Bool? = nil
    ) {
        withLock {
            if let p = permission { if p { _permissionSessions.insert(sessionId) } else { _permissionSessions.remove(sessionId) } }
            if let s = stopped { if s { _stopSessions.insert(sessionId) } else { _stopSessions.remove(sessionId) } }
            if let t = toolRunning { if t { _toolRunningSessions.insert(sessionId) } else { _toolRunningSessions.remove(sessionId) } }
        }
    }

    // MARK: - Hook Configuration

    func ensureHooksConfigured() {
        installNotifyScript()
        updateSettingsJson()
    }

    private func installNotifyScript() {
        let scriptDir = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".codepulse")
        let scriptPath = scriptDir.appendingPathComponent("notify.sh")

        try? FileManager.default.createDirectory(at: scriptDir, withIntermediateDirectories: true)

        let script = """
        #!/bin/bash
        INPUT=$(cat)
        curl -s --max-time 1 -X POST "http://127.0.0.1:\(port)/codepulse-event" \
          -H "Content-Type: application/json" \
          -d "$INPUT" 2>/dev/null &
        exit 0
        """

        try? script.write(to: scriptPath, atomically: true, encoding: .utf8)
        var attrs = [FileAttributeKey: Any]()
        attrs[.posixPermissions] = 0o755
        try? FileManager.default.setAttributes(attrs, ofItemAtPath: scriptPath.path)
    }

    private func updateSettingsJson() {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let settingsURL = home.appendingPathComponent(".claude/settings.json")
        let oldHttpURL = "http://localhost:\(port)/codepulse-event"
        let scriptPath = home.appendingPathComponent(".codepulse/notify.sh").path

        guard FileManager.default.fileExists(atPath: settingsURL.path),
              let data = try? Data(contentsOf: settingsURL),
              var root = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }

        var hooks = root["hooks"] as? [String: Any] ?? [:]
        var changed = false

        // Remove old CodePulse hooks
        for event in hooks.keys {
            guard var entries = hooks[event] as? [[String: Any]] else { continue }
            let before = entries.count
            entries.removeAll { entry in
                guard let entryHooks = entry["hooks"] as? [[String: Any]] else { return false }
                return entryHooks.contains { hook in
                    (hook["url"] as? String) == oldHttpURL
                    || (hook["command"] as? String)?.contains("codepulse") == true
                }
            }
            if entries.count != before {
                hooks[event] = entries.isEmpty ? nil : entries
                changed = true
            }
        }

        // Register all 7 hooks
        let commandHook: [String: Any] = ["type": "command", "command": scriptPath]
        let hookEntry: [String: Any] = ["hooks": [commandHook]]
        let hookEvents = ["Notification", "Stop", "UserPromptSubmit", "PreToolUse", "PostToolUse", "SessionStart", "SessionEnd"]

        for event in hookEvents {
            var entries = hooks[event] as? [[String: Any]] ?? []
            let alreadyPresent = entries.contains { entry in
                guard let entryHooks = entry["hooks"] as? [[String: Any]] else { return false }
                return entryHooks.contains { ($0["command"] as? String) == scriptPath }
            }
            if !alreadyPresent {
                entries.append(hookEntry)
                hooks[event] = entries
                changed = true
            }
        }

        if changed {
            root["hooks"] = hooks
            if let data = try? JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys]) {
                try? data.write(to: settingsURL, options: .atomic)
                logger.info("Updated hooks in settings.json (7 hook events)")
            }
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
            if case .ready = state {
                logger.info("Hook server listening on port \(self.port)")
            }
        }
        listener?.newConnectionHandler = { [weak self] conn in self?.handleConnection(conn) }
        listener?.start(queue: queue)
    }

    func stop() {
        listener?.cancel()
        listener = nil
    }

    // MARK: - Connection

    private func handleConnection(_ connection: NWConnection) {
        connection.start(queue: queue)
        var accumulated = Data()

        func receiveMore() {
            connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
                if let data { accumulated.append(data) }
                if accumulated.count > 512_000 { connection.cancel(); return }

                let done = isComplete || error != nil
                let hasFullRequest = String(data: accumulated, encoding: .utf8)?.contains("\r\n\r\n") ?? false

                if done || hasFullRequest {
                    self?.processRequest(accumulated)
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

        let eventName = json["hook_event_name"] as? String ?? ""
        let sessionId = json["session_id"] as? String
        let toolName = json["tool_name"] as? String
        let cwd = json["cwd"] as? String

        // Track cwd → sessionId mapping from every hook event.
        // This is the only reliable way to know the real sessionId for a project,
        // because session files record the initial ID which becomes stale after /resume.
        if let sessionId, let cwd {
            withLock { _cwdToSessionId[cwd] = sessionId }
        }

        switch eventName {
        case "Notification":
            guard let sessionId else { return }
            let notificationType = json["notification_type"] as? String
                ?? (json["message"] as? [String: Any])?["notification_type"] as? String

            if toolName == "AskUserQuestion" {
                setSessionState(sessionId, permission: true, stopped: false, toolRunning: false)
                emit(sessionId, .askUserQuestion, toolName)
            } else if notificationType == "permission_prompt" {
                setSessionState(sessionId, permission: true, stopped: false, toolRunning: false)
                emit(sessionId, .permissionRequest, toolName)
            } else if notificationType == "elicitation_dialog" {
                setSessionState(sessionId, permission: true, stopped: false)
                emit(sessionId, .elicitationDialog, nil)
            }

        case "Stop":
            guard let sessionId else { return }
            setSessionState(sessionId, permission: false, stopped: true, toolRunning: false)
            emit(sessionId, .stop, nil)

        case "UserPromptSubmit":
            guard let sessionId else { return }
            setSessionState(sessionId, permission: false, stopped: false, toolRunning: false)
            emit(sessionId, .userPromptSubmit, nil)

        case "PreToolUse":
            guard let sessionId else { return }
            setSessionState(sessionId, stopped: false, toolRunning: true)
            emit(sessionId, .preToolUse, toolName)

        case "PostToolUse":
            guard let sessionId else { return }
            setSessionState(sessionId, toolRunning: false)
            emit(sessionId, .postToolUse, toolName)

        case "SessionStart":
            logger.info("SessionStart\(sessionId.map { " \($0.prefix(8))..." } ?? "")")
            DispatchQueue.main.async { [weak self] in self?.onSessionEvent?() }

        case "SessionEnd":
            guard let sessionId else {
                DispatchQueue.main.async { [weak self] in self?.onSessionEvent?() }
                return
            }
            setSessionState(sessionId, permission: false, stopped: false, toolRunning: false)
            logger.info("SessionEnd \(sessionId.prefix(8))...")
            DispatchQueue.main.async { [weak self] in self?.onSessionEvent?() }

        default:
            break
        }
    }

    private func emit(_ sessionId: String, _ signal: HookSignalType, _ toolName: String?) {
        DispatchQueue.main.async { [weak self] in
            self?.onHookSignal?(sessionId, signal, toolName)
            self?.onSessionEvent?()
        }
    }
}
