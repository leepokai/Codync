import Foundation
import CodyncShared
import os

private let logger = Logger(subsystem: "com.pokai.Codync", category: "TranscriptWatcher")

/// Per-session transcript state derived from incremental JSONL parsing.
/// State machine inspired by claude-status (gmr/claude-status-plugin).
struct TranscriptState: Sendable {
    var status: TranscriptStatus = .idle
    var lastEvent: String = ""
    var toolName: String?
    var model: String = "Unknown"
    var latestInputTokens: Int = 0
    var totalOutputTokens: Int = 0
    var lastActivityTime: Date = Date()
    var firstPrompt: String = ""
    var lastPrompt: String = ""

    /// Context usage as percentage. Window size derived from model ID
    /// (e.g., 1M for Opus 4.6+, 200k for older models).
    var contextPct: Int {
        guard latestInputTokens > 0 else { return 0 }
        let contextWindow = ModelInfo.parse(model).contextWindow
        return min(100, (latestInputTokens * 100) / contextWindow)
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
    case compacting
    case idle
}

/// Incrementally parses JSONL transcript files to detect session state.
/// Zero impact on Claude Code — reads files only, no hooks for tool events.
@MainActor
final class TranscriptWatcher {
    /// Per-session tracking (mirrors claude-status's DaemonState)
    private struct SessionTracker {
        var fileOffset: UInt64 = 0
        var lineBuffer: String = ""
        var state: TranscriptState = TranscriptState()

        // Agent tracking: tool_use IDs for Agent tools (subagents)
        var activeAgents: Set<String> = []
        // All active tool_use IDs (for permission timeout heuristic)
        var activeToolIds: Set<String> = []
        var lastToolStartTime: Date?

        // Compaction state: when true, suppress replayed user/progress messages
        var isCompacting: Bool = false

        // Whether this session has received at least one hook event.
        // Sessions that started before hooks were registered need JSONL-based
        // permission timeout as a fallback.
        var hasReceivedHookEvent: Bool = false
    }

    private var trackers: [String: SessionTracker] = [:]
    private let decoder = JSONDecoder()

    // MARK: - Public API

    func state(for sessionId: String) -> TranscriptState? {
        trackers[sessionId]?.state
    }

    /// Receive a hook signal from ClaudeHookServer.
    /// Hook signals are the primary status source — they override JSONL-derived state.
    func handleHookSignal(sessionId: String, signal: HookSignalType, toolName: String? = nil) {
        guard var tracker = trackers[sessionId] else {
            var newTracker = SessionTracker()
            newTracker.state.lastActivityTime = Date()
            applyHookSignal(signal, toolName: toolName, tracker: &newTracker)
            trackers[sessionId] = newTracker
            return
        }
        tracker.state.lastActivityTime = Date()
        applyHookSignal(signal, toolName: toolName, tracker: &tracker)
        trackers[sessionId] = tracker
    }

    private func applyHookSignal(_ signal: HookSignalType, toolName: String?, tracker: inout SessionTracker) {
        tracker.hasReceivedHookEvent = true
        switch signal {
        case .permissionRequest:
            tracker.state.status = .waitingForUser
            tracker.state.lastEvent = toolName.map { "Permission: \($0)" } ?? "Needs permission"
        case .askUserQuestion:
            tracker.state.status = .waitingForUser
            tracker.state.lastEvent = "Waiting for input"
        case .elicitationDialog:
            tracker.state.status = .waitingForUser
            tracker.state.lastEvent = "Waiting for input"
        case .stop:
            tracker.state.status = .idle
            tracker.state.lastEvent = ""
            tracker.activeToolIds.removeAll()
            tracker.activeAgents.removeAll()
            tracker.lastToolStartTime = nil
        case .userPromptSubmit:
            tracker.state.status = .working
            tracker.state.lastEvent = "Processing prompt..."
            tracker.activeToolIds.removeAll()
            tracker.activeAgents.removeAll()
            tracker.lastToolStartTime = nil
        case .preToolUse:
            tracker.state.status = .working
            if let toolName {
                tracker.state.lastEvent = formatToolStatus(name: toolName, input: nil)
            }
        case .postToolUse:
            break
        }
    }

    /// Process new data from a session's JSONL file. Call this on each scan cycle.
    func update(sessionId: String, jsonlURL: URL) {
        var tracker = trackers[sessionId] ?? SessionTracker()

        guard let fileHandle = try? FileHandle(forReadingFrom: jsonlURL) else { return }
        defer { try? fileHandle.close() }

        let fileSize = fileHandle.seekToEndOfFile()

        // First time — skip to end but detect initial status from tail
        if tracker.fileOffset == 0 && trackers[sessionId] == nil {
            tracker.fileOffset = fileSize
            let info = JSONLTailReader.extractInfo(url: jsonlURL)
            tracker.state.model = info.model
            tracker.state.latestInputTokens = info.latestInputTokens
            tracker.state.totalOutputTokens = info.totalOutputTokens
            let (initialStatus, lastEvent) = detectInitialStatus(url: jsonlURL)
            tracker.state.status = initialStatus
            if let lastEvent { tracker.state.lastEvent = lastEvent }
            trackers[sessionId] = tracker
            return
        }

        guard fileSize > tracker.fileOffset else {
            updateTimeouts(&tracker)
            trackers[sessionId] = tracker
            return
        }

        fileHandle.seek(toFileOffset: tracker.fileOffset)
        guard let newData = try? fileHandle.readToEnd(),
              let newText = String(data: newData, encoding: .utf8) else {
            trackers[sessionId] = tracker
            return
        }

        tracker.fileOffset = fileSize
        tracker.state.lastActivityTime = Date()

        let text = tracker.lineBuffer + newText
        var lines = text.components(separatedBy: "\n")
        tracker.lineBuffer = lines.removeLast()

        for line in lines where !line.isEmpty {
            processLine(line, tracker: &tracker)
        }

        trackers[sessionId] = tracker
    }

    func removeSession(_ sessionId: String) {
        trackers.removeValue(forKey: sessionId)
    }

    // MARK: - Line Processing

    private func processLine(_ line: String, tracker: inout SessionTracker) {
        guard let data = line.data(using: .utf8),
              let entry = try? decoder.decode(JSONLTranscriptEntry.self, from: data) else { return }

        // Skip meta messages (plugin commands, local output)
        if entry.isMeta == true { return }

        // Compaction detection via agentId prefix
        if let agentId = entry.agentId, agentId.hasPrefix("acompact-") {
            if tracker.state.status != .compacting {
                tracker.state.status = .compacting
                tracker.state.lastEvent = "Compacting context..."
                tracker.isCompacting = true
            }
            return
        }

        switch entry.type {
        case "assistant":
            processAssistant(entry, tracker: &tracker)
        case "user":
            processUser(entry, tracker: &tracker)
        case "system":
            processSystem(entry, tracker: &tracker)
        case "progress":
            processProgress(entry, tracker: &tracker)
        case "last-prompt":
            if let prompt = entry.lastPrompt, !prompt.isEmpty {
                tracker.state.lastPrompt = String(prompt.prefix(100))
            }
        default:
            break
        }
    }

    private func processAssistant(_ entry: JSONLTranscriptEntry, tracker: inout SessionTracker) {
        guard let message = entry.message else { return }

        // An assistant response clears compaction flag
        if tracker.isCompacting {
            tracker.isCompacting = false
        }

        // Update model and usage
        if let model = message.model, !model.isEmpty {
            tracker.state.model = model
        }
        if let usage = message.usage {
            let contextTokens = usage.totalContextTokens
            if contextTokens > 0 {
                tracker.state.latestInputTokens = contextTokens
            }
            tracker.state.totalOutputTokens += usage.outputTokens ?? 0
        }

        guard let content = message.content else { return }

        // Track tool_use blocks
        let toolUseBlocks = content.filter { $0.type == "tool_use" }
        for block in toolUseBlocks {
            if let id = block.id {
                tracker.activeToolIds.insert(id)
                // Track Agent tool specifically for subagent awareness
                if block.name == "Agent" {
                    tracker.activeAgents.insert(id)
                }
            }
            if let name = block.name {
                tracker.state.toolName = name
                tracker.state.lastEvent = formatToolStatus(name: name, input: block.input)
            }
        }
        if !toolUseBlocks.isEmpty {
            tracker.lastToolStartTime = Date()
        }

        // Determine state from stop_reason (key insight from claude-status)
        switch message.stopReason {
        case nil:
            // Still streaming — active
            tracker.state.status = .working

        case "tool_use":
            // About to execute tools — active
            tracker.state.status = .working

        case "end_turn":
            if !tracker.activeAgents.isEmpty {
                // Subagents still running — stay active
                tracker.state.status = .working
                tracker.state.lastEvent = "Running sub-agent"
            } else {
                // Turn finished — idle
                tracker.state.status = .idle
                tracker.state.lastEvent = ""
                tracker.activeToolIds.removeAll()
                tracker.lastToolStartTime = nil
            }

        case "stop_sequence", "max_tokens":
            tracker.state.status = .idle
            tracker.state.lastEvent = ""
            tracker.activeToolIds.removeAll()
            tracker.activeAgents.removeAll()
            tracker.lastToolStartTime = nil

        default:
            // Unknown stop_reason — treat as still working if we have tool activity
            if !toolUseBlocks.isEmpty || !tracker.activeToolIds.isEmpty {
                tracker.state.status = .working
            }
        }

        // If no stop_reason and has text, still working
        if message.stopReason == nil, content.contains(where: { $0.type == "text" }), toolUseBlocks.isEmpty {
            tracker.state.status = .working
            if tracker.state.lastEvent.isEmpty {
                tracker.state.lastEvent = "Thinking..."
            }
        }
    }

    private func processUser(_ entry: JSONLTranscriptEntry, tracker: inout SessionTracker) {
        // During compaction, user messages are replayed context — skip
        if tracker.isCompacting { return }

        guard let content = entry.message?.content else { return }

        let toolResults = content.filter { $0.type == "tool_result" }

        if !toolResults.isEmpty {
            // Tool completed — remove from tracking
            for result in toolResults {
                if let id = result.toolUseId {
                    tracker.activeToolIds.remove(id)
                    // Only remove from activeAgents if not async
                    if result.isAsync != true {
                        tracker.activeAgents.remove(id)
                    }
                }
            }
            if tracker.activeToolIds.isEmpty {
                tracker.lastToolStartTime = nil
            }
        } else if entry.promptId != nil {
            // Real user prompt (has promptId) — new turn starting
            tracker.state.status = .working
            tracker.state.lastEvent = "Processing prompt..."
            tracker.activeToolIds.removeAll()
            tracker.activeAgents.removeAll()
            tracker.lastToolStartTime = nil

            // Extract prompt text for summary
            if let textBlock = content.first(where: { $0.type == "text" }),
               let text = textBlock.text?.trimmingCharacters(in: .whitespacesAndNewlines),
               !text.isEmpty, !text.hasPrefix("<") {
                let clean = String(text.prefix(100))
                tracker.state.lastPrompt = clean
                if tracker.state.firstPrompt.isEmpty {
                    tracker.state.firstPrompt = clean
                }
            }
        } else {
            // User message without promptId and without tool_result — likely still working
            tracker.state.status = .working
            tracker.state.lastEvent = "Processing..."
        }
    }

    private func processSystem(_ entry: JSONLTranscriptEntry, tracker: inout SessionTracker) {
        switch entry.subtype {
        case "turn_duration", "stop_hook_summary":
            // Turn end signals — stop_hook_summary fires after hooks complete,
            // turn_duration may follow (but not always). Either one marks turn as done.
            tracker.activeAgents.removeAll()
            tracker.activeToolIds.removeAll()
            tracker.lastToolStartTime = nil
            if tracker.state.status == .working || tracker.state.status == .compacting {
                tracker.state.status = .idle
                tracker.state.lastEvent = ""
            }

        case "compact_boundary":
            tracker.state.status = .compacting
            tracker.state.lastEvent = "Compacting context..."
            tracker.isCompacting = true

        default:
            break
        }
    }

    private func processProgress(_ entry: JSONLTranscriptEntry, tracker: inout SessionTracker) {
        // During compaction, progress messages are replayed — skip
        if tracker.isCompacting { return }

        guard let dataType = entry.data?.type else { return }

        switch dataType {
        case "agent_progress":
            tracker.state.status = .working
            tracker.state.lastEvent = "Running sub-agent"
        case "bash_progress":
            tracker.state.status = .working
            tracker.state.lastEvent = "Running command..."
        case "mcp_progress":
            tracker.state.status = .working
            tracker.state.lastEvent = "MCP tool running..."
        case "hook_progress":
            // Hooks running (e.g. notify.sh) — NOT real work, don't change state
            break
        default:
            break
        }
    }

    // MARK: - Timeout Checks

    private func updateTimeouts(_ tracker: inout SessionTracker) {
        let now = Date()
        let sinceLastActivity = now.timeIntervalSince(tracker.state.lastActivityTime)

        // For sessions that started BEFORE hooks were registered (e.g., app just installed),
        // use JSONL-based permission timeout as fallback. Once any hook fires, this is disabled.
        if !tracker.hasReceivedHookEvent,
           !tracker.activeToolIds.isEmpty,
           tracker.activeAgents.isEmpty,
           let toolStart = tracker.lastToolStartTime,
           now.timeIntervalSince(toolStart) > 8.0 {
            if tracker.state.status == .working {
                tracker.state.status = .waitingForUser
                tracker.state.lastEvent = "Needs permission"
            }
        }

        // Safety net: if working with no activity for 30s and no active tools, assume idle.
        // Normally Stop hook handles this instantly — this catches edge cases only.
        if tracker.state.status == .working,
           tracker.activeToolIds.isEmpty,
           tracker.activeAgents.isEmpty,
           sinceLastActivity > 30.0 {
            tracker.state.status = .idle
            tracker.state.lastEvent = ""
        }
    }

    // MARK: - Initial Status Detection

    private func detectInitialStatus(url: URL) -> (TranscriptStatus, String?) {
        let lines = JSONLTailReader.readTail(url: url, lineCount: 20)

        for line in lines.reversed() {
            guard let data = line.data(using: .utf8),
                  let entry = try? decoder.decode(JSONLTranscriptEntry.self, from: data) else { continue }

            if entry.isMeta == true { continue }

            if entry.type == "system" {
                if entry.subtype == "turn_duration" || entry.subtype == "stop_hook_summary" {
                    return (.idle, nil)
                }
                if entry.subtype == "compact_boundary" { return (.compacting, "Compacting context...") }
            }

            if entry.type == "assistant" {
                if let stopReason = entry.message?.stopReason {
                    if stopReason == "end_turn" || stopReason == "stop_sequence" || stopReason == "max_tokens" {
                        return (.idle, nil)
                    }
                }
                if let content = entry.message?.content,
                   let toolBlock = content.first(where: { $0.type == "tool_use" }),
                   let toolName = toolBlock.name {
                    return (.working, formatToolStatus(name: toolName, input: toolBlock.input))
                }
                return (.working, "Thinking...")
            }

            if entry.type == "user" {
                if let content = entry.message?.content,
                   content.contains(where: { $0.type == "tool_result" }) {
                    return (.working, nil)
                }
                return (.working, "Processing prompt...")
            }

            if entry.type == "progress" {
                return (.working, "Running...")
            }
        }

        return (.idle, nil)
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
                return "Running: \(short)\(cmd.count > 40 ? "..." : "")"
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
