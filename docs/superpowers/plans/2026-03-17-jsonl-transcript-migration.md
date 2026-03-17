# JSONL Transcript Migration Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace HTTP hook-based state detection with JSONL transcript incremental parsing, keeping only a single `Notification` command hook for instant permission detection.

**Architecture:** New `TranscriptWatcher` service reads JSONL files incrementally (tracking byte offset per session), parses `assistant/user/system` records to derive state + lastEvent + tool info. `ClaudeHookServer` is simplified to only receive Notification events via a non-blocking shell script hook. All other hooks (Stop, SessionStart, SessionEnd) are removed from `~/.claude/settings.json`.

**Tech Stack:** Swift 6, Foundation (FileHandle for incremental reads), DispatchSource (file change monitoring), NWListener (retained for Notification hook only)

---

## File Structure

| Action | File | Responsibility |
|--------|------|----------------|
| **Create** | `CodePulse-macOS/Services/TranscriptWatcher.swift` | Incremental JSONL parser + state machine per session |
| **Create** | `~/.codepulse/notify.sh` (runtime) | Non-blocking shell hook for Notification events |
| **Modify** | `CodePulse-macOS/Services/SessionStateManager.swift` | Use TranscriptWatcher as primary state source |
| **Modify** | `CodePulse-macOS/Services/ClaudeHookServer.swift` | Strip down to Notification-only receiver + cleanup old hooks |
| **Modify** | `CodePulse-macOS/App/CodePulseApp.swift` | Wire TranscriptWatcher, update init sequence |
| **Modify** | `CodePulse-macOS/Utilities/JSONLTailReader.swift` | Add Codable models for tool_use/tool_result parsing |
| **Modify** | `CLAUDE.md` | Document new architecture |

---

## Chunk 1: TranscriptWatcher — JSONL Incremental Parser

### Task 1: Add JSONL Codable models for transcript parsing

**Files:**
- Modify: `CodePulse-macOS/Utilities/JSONLTailReader.swift`

These models decode the full JSONL record structure needed for state detection.

- [ ] **Step 1: Add content block models to JSONLTailReader.swift**

Add these types below the existing `JSONLEntry` struct:

```swift
// MARK: - Transcript Content Blocks

struct JSONLContentBlock: Codable {
    let type: String           // "text", "tool_use", "tool_result"
    let id: String?            // tool_use id
    let name: String?          // tool name (e.g. "Bash", "Read")
    let input: JSONLToolInput? // tool input (for display)
    let toolUseId: String?     // tool_use_id in tool_result

    enum CodingKeys: String, CodingKey {
        case type, id, name, input
        case toolUseId = "tool_use_id"
    }
}

struct JSONLToolInput: Codable {
    let command: String?
    let filePath: String?
    let pattern: String?

    enum CodingKeys: String, CodingKey {
        case command
        case filePath = "file_path"
        case pattern
    }
}

struct JSONLTranscriptMessage: Codable {
    let role: String?
    let model: String?
    let usage: JSONLUsage?
    let content: [JSONLContentBlock]?
    let stopReason: String?

    enum CodingKeys: String, CodingKey {
        case role, model, usage, content
        case stopReason = "stop_reason"
    }
}

struct JSONLTranscriptEntry: Codable {
    let type: String?         // "assistant", "user", "system"
    let subtype: String?      // "turn_duration" for system entries
    let message: JSONLTranscriptMessage?
    let timestamp: String?
}
```

- [ ] **Step 2: Verify the file compiles**

Run: `cd /Users/libokai/mycode/swiftui/CodePulse && xcodebuild -scheme CodePulse-macOS -destination 'platform=macOS' build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add CodePulse-macOS/Utilities/JSONLTailReader.swift
git commit -m "feat: add JSONL transcript content block models for state parsing"
```

---

### Task 2: Create TranscriptWatcher service

**Files:**
- Create: `CodePulse-macOS/Services/TranscriptWatcher.swift`

This is the core new service. It tracks byte offset per session, reads new JSONL lines incrementally, and maintains a state machine.

- [ ] **Step 1: Create TranscriptWatcher.swift**

```swift
import Foundation
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "TranscriptWatcher")

/// Per-session transcript state derived from incremental JSONL parsing
struct TranscriptState: Sendable {
    var status: TranscriptStatus = .idle
    var lastEvent: String = ""
    var toolName: String?
    var model: String = "Unknown"
    var latestInputTokens: Int = 0
    var totalOutputTokens: Int = 0
    var lastActivityTime: Date = Date()

    var contextPct: Int {
        guard latestInputTokens > 0 else { return 0 }
        return min(100, (latestInputTokens * 100) / 200_000)
    }

    var costUSD: Double {
        let (inputRate, outputRate) = modelPricing
        return Double(latestInputTokens) * inputRate / 1_000_000
             + Double(totalOutputTokens) * outputRate / 1_000_000
    }

    private var modelPricing: (Double, Double) {
        if model.contains("opus") { return (15.0, 75.0) }
        if model.contains("sonnet") { return (3.0, 15.0) }
        if model.contains("haiku") { return (0.25, 1.25) }
        return (3.0, 15.0)
    }
}

enum TranscriptStatus: Sendable {
    case working
    case waitingForUser
    case idle
}

/// Incrementally parses JSONL transcript files to detect session state.
/// Replaces hook-based detection — zero impact on Claude Code.
@MainActor
final class TranscriptWatcher {
    /// Per-session tracking
    private struct SessionTracker {
        var fileOffset: UInt64 = 0
        var lineBuffer: String = ""
        var state: TranscriptState = TranscriptState()
        var activeToolIds: Set<String> = []
        var hadToolsInTurn: Bool = false
        var lastToolStartTime: Date?
    }

    private var trackers: [String: SessionTracker] = [:]
    private let decoder = JSONDecoder()

    /// Permission timeout — if a tool starts but no result comes back within this interval,
    /// assume Claude is waiting for permission approval.
    private let permissionTimeoutSeconds: TimeInterval = 8.0

    /// Idle timeout — if no new data after turn ends, consider session idle.
    private let idleTimeoutSeconds: TimeInterval = 10.0

    // MARK: - Public API

    /// Get current transcript state for a session. Returns nil if not yet tracked.
    func state(for sessionId: String) -> TranscriptState? {
        trackers[sessionId]?.state
    }

    /// Process new data from a session's JSONL file. Call this on each scan cycle.
    func update(sessionId: String, jsonlURL: URL) {
        var tracker = trackers[sessionId] ?? SessionTracker()

        // Read new bytes from the file
        guard let fileHandle = try? FileHandle(forReadingFrom: jsonlURL) else { return }
        defer { try? fileHandle.close() }

        let fileSize = fileHandle.seekToEndOfFile()

        // First time seeing this session — skip to end (don't replay history)
        if tracker.fileOffset == 0 && trackers[sessionId] == nil {
            tracker.fileOffset = fileSize
            // Still extract model/cost from tail for initial display
            let info = JSONLTailReader.extractInfo(url: jsonlURL)
            tracker.state.model = info.model
            tracker.state.latestInputTokens = info.latestInputTokens
            tracker.state.totalOutputTokens = info.totalOutputTokens
            trackers[sessionId] = tracker
            return
        }

        guard fileSize > tracker.fileOffset else {
            // No new data — check timeouts
            updateTimeouts(&tracker)
            trackers[sessionId] = tracker
            return
        }

        // Read new bytes
        fileHandle.seek(toFileOffset: tracker.fileOffset)
        guard let newData = try? fileHandle.readToEnd(),
              let newText = String(data: newData, encoding: .utf8) else {
            trackers[sessionId] = tracker
            return
        }

        tracker.fileOffset = fileSize
        tracker.state.lastActivityTime = Date()

        // Parse lines (handle partial lines with lineBuffer)
        let text = tracker.lineBuffer + newText
        var lines = text.components(separatedBy: "\n")
        tracker.lineBuffer = lines.removeLast() // May be incomplete

        for line in lines where !line.isEmpty {
            processLine(line, tracker: &tracker)
        }

        trackers[sessionId] = tracker
    }

    /// Remove tracking for a session (when it completes)
    func removeSession(_ sessionId: String) {
        trackers.removeValue(forKey: sessionId)
    }

    // MARK: - Line Processing

    private func processLine(_ line: String, tracker: inout SessionTracker) {
        guard let data = line.data(using: .utf8),
              let entry = try? decoder.decode(JSONLTranscriptEntry.self, from: data) else { return }

        switch entry.type {
        case "assistant":
            processAssistant(entry, tracker: &tracker)
        case "user":
            processUser(entry, tracker: &tracker)
        case "system":
            processSystem(entry, tracker: &tracker)
        default:
            break
        }
    }

    private func processAssistant(_ entry: JSONLTranscriptEntry, tracker: inout SessionTracker) {
        guard let message = entry.message else { return }

        // Update model and usage
        if let model = message.model, !model.isEmpty {
            tracker.state.model = model
        }
        if let usage = message.usage {
            let inputTokens = usage.inputTokens ?? 0
            let cacheRead = usage.cacheReadInputTokens ?? 0
            if inputTokens > 0 {
                tracker.state.latestInputTokens = inputTokens + cacheRead
            }
            tracker.state.totalOutputTokens += usage.outputTokens ?? 0
        }

        // Check content blocks for tool_use
        guard let content = message.content else { return }
        let toolUseBlocks = content.filter { $0.type == "tool_use" }

        if !toolUseBlocks.isEmpty {
            tracker.state.status = .working
            tracker.hadToolsInTurn = true

            for block in toolUseBlocks {
                if let id = block.id {
                    tracker.activeToolIds.insert(id)
                }
                if let name = block.name {
                    tracker.state.toolName = name
                    tracker.state.lastEvent = formatToolStatus(name: name, input: block.input)
                }
            }
            tracker.lastToolStartTime = Date()
        } else if content.contains(where: { $0.type == "text" }) {
            // Text-only response — agent is working (generating text)
            tracker.state.status = .working
            tracker.state.lastEvent = "Thinking..."
        }
    }

    private func processUser(_ entry: JSONLTranscriptEntry, tracker: inout SessionTracker) {
        guard let content = entry.message?.content else { return }

        let toolResults = content.filter { $0.type == "tool_result" }

        if !toolResults.isEmpty {
            // Tool completed
            for result in toolResults {
                if let id = result.toolUseId {
                    tracker.activeToolIds.remove(id)
                }
            }
            if tracker.activeToolIds.isEmpty {
                tracker.hadToolsInTurn = false
                tracker.lastToolStartTime = nil
            }
        } else {
            // New user prompt — new turn starting
            tracker.state.status = .working
            tracker.state.lastEvent = "Processing prompt..."
            tracker.activeToolIds.removeAll()
            tracker.hadToolsInTurn = false
            tracker.lastToolStartTime = nil
        }
    }

    private func processSystem(_ entry: JSONLTranscriptEntry, tracker: inout SessionTracker) {
        if entry.subtype == "turn_duration" {
            // Definitive turn end — agent finished, waiting for user
            tracker.state.status = .waitingForUser
            tracker.state.lastEvent = "Waiting for input"
            tracker.activeToolIds.removeAll()
            tracker.hadToolsInTurn = false
            tracker.lastToolStartTime = nil
        }
    }

    // MARK: - Timeout Checks

    private func updateTimeouts(_ tracker: inout SessionTracker) {
        let now = Date()

        // Permission heuristic: tool started but no result after timeout
        if !tracker.activeToolIds.isEmpty,
           let toolStart = tracker.lastToolStartTime,
           now.timeIntervalSince(toolStart) > permissionTimeoutSeconds {
            // Don't override if already waiting for user (turn ended)
            if tracker.state.status == .working {
                tracker.state.status = .waitingForUser
                tracker.state.lastEvent = "Needs permission"
            }
        }

        // Idle timeout: no new data for a while after waiting
        if tracker.state.status == .waitingForUser,
           now.timeIntervalSince(tracker.state.lastActivityTime) > idleTimeoutSeconds {
            tracker.state.status = .idle
        }
    }

    // MARK: - Display Formatting

    private func formatToolStatus(name: String, input: JSONLToolInput?) -> String {
        switch name {
        case "Read":
            if let fp = input?.filePath { return "Reading \(URL(fileURLWithPath: fp).lastPathComponent)" }
            return "Reading file"
        case "Edit":
            if let fp = input?.filePath { return "Editing \(URL(fileURLWithPath: fp).lastPathComponent)" }
            return "Editing file"
        case "Write":
            if let fp = input?.filePath { return "Writing \(URL(fileURLWithPath: fp).lastPathComponent)" }
            return "Writing file"
        case "Bash":
            if let cmd = input?.command {
                let short = cmd.prefix(40)
                return "Running: \(short)\(cmd.count > 40 ? "…" : "")"
            }
            return "Running command"
        case "Glob":
            return "Searching files"
        case "Grep":
            return "Searching code"
        case "Agent":
            return "Running sub-agent"
        case "WebFetch":
            return "Fetching web content"
        case "WebSearch":
            return "Searching the web"
        default:
            return "Using \(name)"
        }
    }
}
```

- [ ] **Step 2: Add TranscriptWatcher.swift to the Xcode project**

Open `CodePulse.xcodeproj/project.pbxproj` and add the file to the macOS target's Sources build phase. Alternatively, if using xcodegen, add to `project.yml` and regenerate.

- [ ] **Step 3: Verify it compiles**

Run: `cd /Users/libokai/mycode/swiftui/CodePulse && xcodebuild -scheme CodePulse-macOS -destination 'platform=macOS' build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 4: Commit**

```bash
git add CodePulse-macOS/Services/TranscriptWatcher.swift CodePulse.xcodeproj/project.pbxproj
git commit -m "feat: add TranscriptWatcher for JSONL incremental state detection"
```

---

## Chunk 2: Simplify HookServer + Migrate SessionStateManager

### Task 3: Simplify ClaudeHookServer to Notification-only + command hook

**Files:**
- Modify: `CodePulse-macOS/Services/ClaudeHookServer.swift`

Strip the hook server down to:
1. Only register a single `Notification` hook using `"type": "command"` (non-blocking shell script)
2. Clean up ALL old HTTP hooks from settings.json
3. Keep the NWListener for receiving events from the script
4. Create the notify.sh script at `~/.codepulse/notify.sh`

- [ ] **Step 1: Rewrite ClaudeHookServer.swift**

Replace the entire file with this simplified version:

```swift
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
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/libokai/mycode/swiftui/CodePulse && xcodebuild -scheme CodePulse-macOS -destination 'platform=macOS' build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add CodePulse-macOS/Services/ClaudeHookServer.swift
git commit -m "refactor: simplify HookServer to Notification-only with command hook"
```

---

### Task 4: Migrate SessionStateManager to use TranscriptWatcher

**Files:**
- Modify: `CodePulse-macOS/Services/SessionStateManager.swift`

Replace hook-based state detection with TranscriptWatcher. The hook server is now only consulted for permission detection.

- [ ] **Step 1: Rewrite SessionStateManager.swift**

Replace the entire file:

```swift
import Foundation
import Combine
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "StateManager")

@MainActor
final class SessionStateManager: ObservableObject {
    @Published var sessions: [SessionState] = []

    private let scanner: SessionScanner
    let transcriptWatcher = TranscriptWatcher()
    var hookServer: ClaudeHookServer?

    private var cancellables = Set<AnyCancellable>()
    private let deviceId = Host.current().localizedName ?? UUID().uuidString

    init(scanner: SessionScanner) {
        self.scanner = scanner
        scanner.$activeSessions
            .debounce(for: .seconds(0.5), scheduler: RunLoop.main)
            .sink { [weak self] rawSessions in
                self?.updateSessions(from: rawSessions)
            }
            .store(in: &cancellables)
    }

    private func updateSessions(from rawSessions: [String: RawSessionFile]) {
        var updated: [SessionState] = []

        for (sessionId, raw) in rawSessions {
            // Parse JSONL incrementally — this is now the primary state source
            let jsonlUrl = ClaudePaths.jsonlPath(cwd: raw.cwd, sessionId: sessionId)
            transcriptWatcher.update(sessionId: sessionId, jsonlURL: jsonlUrl)

            let transcript = transcriptWatcher.state(for: sessionId)
            let tasks = SessionFileParser.parseTasks(sessionId: sessionId)
            let indexEntry = SessionFileParser.parseSessionIndex(cwd: raw.cwd, sessionId: sessionId)

            // Permission: instant detection from hook, clear when transcript shows activity
            let needsPermission = hookServer?.needsPermission(sessionId) ?? false
            if !needsPermission {
                // No-op: permission not needed
            } else if let t = transcript, t.status == .working {
                // JSONL shows activity resumed — clear permission flag
                hookServer?.clearPermission(sessionId)
            }

            let status = detectStatus(
                raw: raw,
                transcript: transcript,
                tasks: tasks,
                needsPermission: needsPermission
            )

            let projectName = URL(fileURLWithPath: raw.cwd).lastPathComponent
            let startDate = Date(timeIntervalSince1970: TimeInterval(raw.startedAt) / 1000)
            let duration = Int(Date().timeIntervalSince(startDate))
            let currentTask = tasks.first(where: { $0.status == .inProgress })?.activeForm
            let lastEvent = transcript?.lastEvent

            let summary = indexEntry?.summary
                ?? indexEntry?.firstPrompt.map { String($0.prefix(50)) }
                ?? projectName

            let existingSession = sessions.first { $0.sessionId == sessionId }
            let contentChanged = existingSession == nil
                || existingSession?.status != status
                || existingSession?.tasks != tasks
                || existingSession?.contextPct != (transcript?.contextPct ?? 0)
                || existingSession?.currentTask != currentTask
                || existingSession?.lastEvent != lastEvent

            let updatedAt = contentChanged ? Date() : (existingSession?.updatedAt ?? Date())

            let session = SessionState(
                sessionId: sessionId,
                projectName: projectName,
                gitBranch: indexEntry?.gitBranch ?? "unknown",
                status: status,
                model: formatModel(transcript?.model ?? "Unknown"),
                summary: summary,
                currentTask: currentTask,
                lastEvent: lastEvent,
                tasks: tasks,
                contextPct: transcript?.contextPct ?? 0,
                costUSD: transcript?.costUSD ?? 0,
                startedAt: startDate,
                durationSec: duration,
                deviceId: deviceId,
                updatedAt: updatedAt
            )
            updated.append(session)
        }

        // Mark disappeared sessions as completed
        for existing in sessions where existing.status != .completed {
            if !rawSessions.keys.contains(existing.sessionId) {
                var completed = existing
                completed.status = .completed
                completed.updatedAt = Date()
                updated.append(completed)
                transcriptWatcher.removeSession(existing.sessionId)
            }
        }

        let newSessions = updated.sorted { $0.startedAt > $1.startedAt }
        if newSessions != sessions {
            sessions = newSessions
        }
    }

    private func detectStatus(
        raw: RawSessionFile,
        transcript: TranscriptState?,
        tasks: [TaskItem],
        needsPermission: Bool
    ) -> SessionStatus {
        // 1. PID dead → completed
        guard PIDChecker.isAlive(pid: raw.pid) else { return .completed }

        // 2. Hook says needs permission (instant, from Notification hook)
        if needsPermission { return .needsInput }

        // 3. Transcript state (from JSONL parsing)
        if let transcript {
            switch transcript.status {
            case .working:
                return .working
            case .waitingForUser:
                return .needsInput
            case .idle:
                return .idle
            }
        }

        // 4. Fallback: check tasks
        if tasks.contains(where: { $0.status == .inProgress }) { return .working }

        // 5. No signal → idle
        return .idle
    }

    private func formatModel(_ raw: String) -> String {
        if raw.contains("opus") { return "Opus" }
        if raw.contains("sonnet") { return "Sonnet" }
        if raw.contains("haiku") { return "Haiku" }
        return raw
    }
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd /Users/libokai/mycode/swiftui/CodePulse && xcodebuild -scheme CodePulse-macOS -destination 'platform=macOS' build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add CodePulse-macOS/Services/SessionStateManager.swift
git commit -m "refactor: migrate SessionStateManager to TranscriptWatcher + permission hook"
```

---

### Task 5: Update AppDelegate wiring

**Files:**
- Modify: `CodePulse-macOS/App/CodePulseApp.swift`

Wire the new TranscriptWatcher-based flow. Hook server now only handles Notification.

- [ ] **Step 1: Update CodePulseApp.swift**

Replace the `AppDelegate` class:

```swift
@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    var menuBarController: MenuBarController?
    let scanner = SessionScanner()
    var stateManager: SessionStateManager!
    var cloudKitSync: CloudKitSync!
    let hookServer = ClaudeHookServer()

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)

        stateManager = SessionStateManager(scanner: scanner)
        stateManager.hookServer = hookServer
        cloudKitSync = CloudKitSync(stateManager: stateManager)
        menuBarController = MenuBarController(stateManager: stateManager)

        // Notification-only hook for instant permission detection
        hookServer.ensureHooksConfigured()
        hookServer.onPermissionEvent = { [weak self] in
            self?.scanner.scan() // Trigger immediate rescan → state manager picks up permission
        }
        hookServer.start()

        scanner.start()
        logger.info("CodePulse launched — JSONL transcript watcher + notification hook active")
    }

    func applicationWillTerminate(_ notification: Notification) {
        scanner.stop()
        hookServer.stop()
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}
```

- [ ] **Step 2: Verify it compiles and runs**

Run: `cd /Users/libokai/mycode/swiftui/CodePulse && xcodebuild -scheme CodePulse-macOS -destination 'platform=macOS' build 2>&1 | tail -5`
Expected: BUILD SUCCEEDED

- [ ] **Step 3: Commit**

```bash
git add CodePulse-macOS/App/CodePulseApp.swift
git commit -m "refactor: update AppDelegate for transcript watcher + notification hook"
```

---

## Chunk 3: Cleanup + Documentation

### Task 6: Clean up old hooks from current settings.json

**Files:**
- No code change — manual verification step

- [ ] **Step 1: Verify settings.json was cleaned up**

After building and running the app once, verify `~/.claude/settings.json`:

```bash
cat ~/.claude/settings.json | python3 -m json.tool
```

Expected: Only a single `Notification` hook with `"type": "command"` pointing to `~/.codepulse/notify.sh`. No HTTP hooks with `localhost:19221`.

- [ ] **Step 2: Verify notify.sh exists and is executable**

```bash
ls -la ~/.codepulse/notify.sh
cat ~/.codepulse/notify.sh
```

Expected: Script exists, is executable (755), contains `curl -s --max-time 1` with `|| true`.

---

### Task 7: Update CLAUDE.md

**Files:**
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update CLAUDE.md architecture section**

Replace the Architecture section:

```markdown
## Architecture

- macOS menu bar app using `NSStatusItem` + `NSPopover`
- **JSONL transcript parsing** (like Pixel Agents) for state detection — zero impact on Claude Code
- Single `Notification` command hook (`~/.codepulse/notify.sh`) for instant permission detection only
- Shell script hook uses `curl --max-time 1 || true` — never blocks Claude Code even if app is not running
- Shared code in `CodePulseShared` Swift Package (macOS 14+ / iOS 17+)
- **Do NOT register HTTP hooks** — they block Claude Code on ECONNREFUSED when app is down
- **Do NOT register PreToolUse or PostToolUse hooks** — they are synchronous and block tool execution
```

- [ ] **Step 2: Commit**

```bash
git add CLAUDE.md
git commit -m "docs: update CLAUDE.md for JSONL transcript architecture"
```

---

### Task 8: Remove unused JSONLTailReader code (optional cleanup)

**Files:**
- Modify: `CodePulse-macOS/Utilities/JSONLTailReader.swift`

The old `extractInfo()` method is still used for initial session bootstrapping in TranscriptWatcher. The old `readTail()` is still used by `extractInfo()`. Both can stay — they still serve a purpose for catching up on existing sessions.

- [ ] **Step 1: Verify no dead code**

Check that `JSONLTailReader.extractInfo()`, `readTail()`, and `fileSize()` are all still referenced:

```bash
grep -r "JSONLTailReader" CodePulse-macOS/
```

Expected: References in `TranscriptWatcher.swift` (extractInfo for bootstrap) and possibly `SessionStateManager.swift` (removed — now uses transcript).

- [ ] **Step 2: Remove fileSize and jsonlGrowth tracking if no longer used**

If `fileSize()` is no longer called anywhere (the old JSONL growth detection was in SessionStateManager which we rewrote), remove it.

- [ ] **Step 3: Commit if changes made**

```bash
git add CodePulse-macOS/Utilities/JSONLTailReader.swift
git commit -m "chore: remove unused JSONLTailReader.fileSize()"
```
