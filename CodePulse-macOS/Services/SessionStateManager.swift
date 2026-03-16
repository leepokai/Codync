import Foundation
import Combine
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "StateManager")

@MainActor
final class SessionStateManager: ObservableObject {
    @Published var sessions: [SessionState] = []

    private let scanner: SessionScanner
    var hookServer: ClaudeHookServer?

    private var cancellables = Set<AnyCancellable>()
    private let deviceId = Host.current().localizedName ?? UUID().uuidString

    /// Track JSONL file sizes to detect growth (= session is active)
    private var jsonlSizes: [String: UInt64] = [:]
    private var jsonlLastGrowth: [String: Date] = [:]

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
        let hookSessions = hookServer?.sessions ?? [:]
        var updated: [SessionState] = []

        for (sessionId, raw) in rawSessions {
            let tasks = SessionFileParser.parseTasks(sessionId: sessionId)
            let indexEntry = SessionFileParser.parseSessionIndex(cwd: raw.cwd, sessionId: sessionId)
            let jsonlUrl = ClaudePaths.jsonlPath(cwd: raw.cwd, sessionId: sessionId)
            let jsonlInfo = JSONLTailReader.extractInfo(url: jsonlUrl)
            let hookInfo = hookSessions[sessionId]

            // Track JSONL file growth
            let jsonlGrowing = updateJsonlGrowth(sessionId: sessionId, url: jsonlUrl)

            let status = detectStatus(sessionId: sessionId, raw: raw, tasks: tasks, hookState: hookInfo?.state, jsonlGrowing: jsonlGrowing)

            let projectName = URL(fileURLWithPath: raw.cwd).lastPathComponent
            let startDate = Date(timeIntervalSince1970: TimeInterval(raw.startedAt) / 1000)
            let duration = Int(Date().timeIntervalSince(startDate))
            let currentTask = tasks.first(where: { $0.status == .inProgress })?.activeForm
            let lastEvent = hookInfo?.lastEvent

            let summary = indexEntry?.summary
                ?? indexEntry?.firstPrompt.map { String($0.prefix(50)) }
                ?? projectName

            let existingSession = sessions.first { $0.sessionId == sessionId }
            let contentChanged = existingSession == nil
                || existingSession?.status != status
                || existingSession?.tasks != tasks
                || existingSession?.contextPct != jsonlInfo.contextPct
                || existingSession?.currentTask != currentTask
                || existingSession?.lastEvent != lastEvent

            let updatedAt = contentChanged ? Date() : (existingSession?.updatedAt ?? Date())

            let session = SessionState(
                sessionId: sessionId,
                projectName: projectName,
                gitBranch: indexEntry?.gitBranch ?? "unknown",
                status: status,
                model: formatModel(jsonlInfo.model),
                summary: summary,
                currentTask: currentTask,
                lastEvent: lastEvent,
                tasks: tasks,
                contextPct: jsonlInfo.contextPct,
                costUSD: jsonlInfo.costUSD,
                startedAt: startDate,
                durationSec: duration,
                deviceId: deviceId,
                updatedAt: updatedAt
            )
            updated.append(session)
        }

        for existing in sessions where existing.status != .completed {
            if !rawSessions.keys.contains(existing.sessionId) {
                var completed = existing
                completed.status = .completed
                completed.updatedAt = Date()
                updated.append(completed)
            }
        }

        let newSessions = updated.sorted { $0.startedAt > $1.startedAt }
        if newSessions != sessions {
            sessions = newSessions
        }
    }

    /// Returns true if JSONL file has grown recently (within last 10 seconds)
    private func updateJsonlGrowth(sessionId: String, url: URL) -> Bool {
        let currentSize = JSONLTailReader.fileSize(url: url) ?? 0
        let previousSize = jsonlSizes[sessionId] ?? 0

        if currentSize != previousSize {
            jsonlSizes[sessionId] = currentSize
            jsonlLastGrowth[sessionId] = Date()
            return true
        }

        // Consider "growing" if file changed in last 10 seconds
        if let lastGrowth = jsonlLastGrowth[sessionId] {
            return Date().timeIntervalSince(lastGrowth) < 10
        }

        // First time seeing this session — record size, don't report as growing
        jsonlSizes[sessionId] = currentSize
        return false
    }

    private func detectStatus(sessionId: String, raw: RawSessionFile, tasks: [TaskItem], hookState: ClaudeState?, jsonlGrowing: Bool) -> SessionStatus {
        // 1. PID dead → completed
        guard PIDChecker.isAlive(pid: raw.pid) else { return .completed }

        // 2. Hook state is the primary source of truth
        if let hookState {
            switch hookState {
            case .working:
                return .working
            case .waitingForUser, .needsPermission:
                // BUT if JSONL is growing, Claude is actually responding
                // (user sent message or Claude is generating text without tools)
                if jsonlGrowing {
                    return .working
                }
                return hookState == .needsPermission ? .needsInput : .needsInput
            }
        }

        // 3. JSONL growing = something is happening
        if jsonlGrowing { return .working }

        // 4. Fallback: check tasks
        if tasks.contains(where: { $0.status == .inProgress }) { return .working }

        // 5. No signal at all → idle
        return .idle
    }

    private func formatModel(_ raw: String) -> String {
        if raw.contains("opus") { return "Opus" }
        if raw.contains("sonnet") { return "Sonnet" }
        if raw.contains("haiku") { return "Haiku" }
        return raw
    }
}
