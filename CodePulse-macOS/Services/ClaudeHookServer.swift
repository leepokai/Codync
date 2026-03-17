import Foundation
import Network
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "HookServer")

/// Minimal hook server — only receives Notification events for instant permission detection.
/// Uses a command hook (shell script) instead of HTTP hook to avoid blocking Claude Code.
final class ClaudeHookServer: @unchecked Sendable {
    private var listener: NWListener?
    private let port: UInt16
    private let queue = DispatchQueue(label: "com.pokai.CodePulse.hooks", qos: .utility)
    private let lock = NSLock()

    private var _permissionSessions: Set<String> = []

    /// Sessions currently waiting for permission (thread-safe)
    var permissionSessions: Set<String> {
        lock.lock()
        defer { lock.unlock() }
        return _permissionSessions
    }

    /// Check if a specific session needs permission
    func needsPermission(_ sessionId: String) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return _permissionSessions.contains(sessionId)
    }

    /// Clear permission state (called when JSONL shows activity resumed)
    func clearPermission(_ sessionId: String) {
        lock.lock()
        _permissionSessions.remove(sessionId)
        lock.unlock()
    }

    var onPermissionEvent: (@Sendable () -> Void)?

    init(port: UInt16 = 19221) {
        self.port = port
    }

    // MARK: - Hook Configuration

    /// Install notify.sh and register a single Notification command hook.
    /// Also cleans up ALL old CodePulse HTTP hooks from settings.json.
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
        # CodePulse: non-blocking hook notification
        # If CodePulse isn't running, curl fails silently — Claude Code is unaffected
        curl -s --max-time 1 -X POST "http://127.0.0.1:\(port)/codepulse-event" \
          -H "Content-Type: application/json" \
          -d "$HOOK_INPUT" 2>/dev/null || true
        """

        try? script.write(to: scriptPath, atomically: true, encoding: .utf8)

        // Make executable
        var attrs = [FileAttributeKey: Any]()
        attrs[.posixPermissions] = 0o755
        try? FileManager.default.setAttributes(attrs, ofItemAtPath: scriptPath.path)

        logger.info("Installed notify.sh at \(scriptPath.path)")
    }

    private func updateSettingsJson() {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let settingsURL = home.appendingPathComponent(".claude/settings.json")
        let oldHttpURL = "http://localhost:\(port)/codepulse-event"
        let scriptPath = home.appendingPathComponent(".codepulse/notify.sh").path

        guard FileManager.default.fileExists(atPath: settingsURL.path),
              let data = try? Data(contentsOf: settingsURL),
              var root = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            logger.warning("Could not read ~/.claude/settings.json")
            return
        }

        var hooks = root["hooks"] as? [String: Any] ?? [:]
        var changed = false

        // 1. Remove ALL old CodePulse HTTP hooks from every event type
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

        // 2. Register single Notification command hook
        let commandHook: [String: Any] = ["type": "command", "command": scriptPath]
        var notifEntries = hooks["Notification"] as? [[String: Any]] ?? []
        let alreadyPresent = notifEntries.contains { entry in
            guard let entryHooks = entry["hooks"] as? [[String: Any]] else { return false }
            return entryHooks.contains { ($0["command"] as? String) == scriptPath }
        }
        if !alreadyPresent {
            notifEntries.append(["hooks": [commandHook]])
            hooks["Notification"] = notifEntries
            changed = true
        }

        if changed {
            root["hooks"] = hooks
            if let data = try? JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys]) {
                try? data.write(to: settingsURL, options: .atomic)
                logger.info("Updated hooks in settings.json (Notification command hook only)")
            }
        }
    }

    // MARK: - Server (receives events from notify.sh)

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
                logger.info("Notification hook server listening on port \(self.port)")
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
    }

    // MARK: - Connection

    private func handleConnection(_ connection: NWConnection) {
        connection.start(queue: queue)

        var accumulated = Data()

        func receiveMore() {
            connection.receive(minimumIncompleteLength: 1, maximumLength: 65536) { [weak self] data, _, isComplete, error in
                if let data { accumulated.append(data) }

                if accumulated.count > 512_000 {
                    connection.cancel()
                    return
                }

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
        guard eventName == "Notification" else { return } // Only care about Notification

        let sessionId = json["session_id"] as? String
        guard let sessionId else { return }

        let notificationType = json["notification_type"] as? String
            ?? (json["message"] as? [String: Any])?["notification_type"] as? String

        if notificationType == "permission_prompt" {
            lock.lock()
            _permissionSessions.insert(sessionId)
            lock.unlock()
            logger.debug("Permission needed for session \(sessionId.prefix(8))...")
            DispatchQueue.main.async { [weak self] in self?.onPermissionEvent?() }
        }
    }
}
