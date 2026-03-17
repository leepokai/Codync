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
        let sinceLastActivity = now.timeIntervalSince(tracker.state.lastActivityTime)

        // Permission heuristic: tool started but no result after timeout
        if !tracker.activeToolIds.isEmpty,
           let toolStart = tracker.lastToolStartTime,
           now.timeIntervalSince(toolStart) > permissionTimeoutSeconds {
            if tracker.state.status == .working {
                tracker.state.status = .waitingForUser
                tracker.state.lastEvent = "Needs permission"
            }
        }

        // Working but no new data → turn_duration was missed or hasn't arrived yet
        if tracker.state.status == .working,
           tracker.activeToolIds.isEmpty,
           sinceLastActivity > idleTimeoutSeconds {
            tracker.state.status = .waitingForUser
            tracker.state.lastEvent = "Waiting for input"
        }

        // Waiting but no new data for even longer → truly idle
        if tracker.state.status == .waitingForUser,
           sinceLastActivity > idleTimeoutSeconds * 3 {
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
