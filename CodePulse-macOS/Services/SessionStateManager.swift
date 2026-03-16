import Foundation
import Combine
import CodePulseShared
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "StateManager")

@MainActor
final class SessionStateManager: ObservableObject {
    @Published var sessions: [SessionState] = []

    private let scanner: SessionScanner
    /// Reference to hook server for real-time Claude state
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
        let hookStates = hookServer?.sessionStates ?? [:]
        var updated: [SessionState] = []

        for (sessionId, raw) in rawSessions {
            let tasks = SessionFileParser.parseTasks(sessionId: sessionId)
            let indexEntry = SessionFileParser.parseSessionIndex(cwd: raw.cwd, sessionId: sessionId)
            let jsonlUrl = ClaudePaths.jsonlPath(cwd: raw.cwd, sessionId: sessionId)
            let jsonlInfo = JSONLTailReader.extractInfo(url: jsonlUrl)

            // Use hook state as primary source of truth (like Command app)
            let status = detectStatus(sessionId: sessionId, raw: raw, tasks: tasks, hookState: hookStates[sessionId])

            let projectName = URL(fileURLWithPath: raw.cwd).lastPathComponent
            let startDate = Date(timeIntervalSince1970: TimeInterval(raw.startedAt) / 1000)
            let duration = Int(Date().timeIntervalSince(startDate))
            let currentTask = tasks.first(where: { $0.status == .inProgress })?.activeForm

            let summary = indexEntry?.summary
                ?? indexEntry?.firstPrompt.map { String($0.prefix(50)) }
                ?? projectName

            let existingSession = sessions.first { $0.sessionId == sessionId }
            let contentChanged = existingSession == nil
                || existingSession?.status != status
                || existingSession?.tasks != tasks
                || existingSession?.contextPct != jsonlInfo.contextPct
                || existingSession?.currentTask != currentTask

            let updatedAt = contentChanged ? Date() : (existingSession?.updatedAt ?? Date())

            let session = SessionState(
                sessionId: sessionId,
                projectName: projectName,
                gitBranch: indexEntry?.gitBranch ?? "unknown",
                status: status,
                model: formatModel(jsonlInfo.model),
                summary: summary,
                currentTask: currentTask,
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

        // Mark recently disappeared sessions as completed
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

    private func detectStatus(sessionId: String, raw: RawSessionFile, tasks: [TaskItem], hookState: ClaudeState?) -> SessionStatus {
        // 1. PID dead → completed
        guard PIDChecker.isAlive(pid: raw.pid) else { return .completed }

        // 2. Hook state is the primary source of truth (real-time from Claude Code)
        if let hookState {
            switch hookState {
            case .working: return .working
            case .waitingForUser: return .needsInput
            case .needsPermission: return .needsInput
            }
        }

        // 3. Fallback: check tasks (for sessions started before CodePulse)
        if tasks.contains(where: { $0.status == .inProgress }) { return .working }

        // 4. No hook data, no active tasks → idle
        return .idle
    }

    private func formatModel(_ raw: String) -> String {
        if raw.contains("opus") { return "Opus" }
        if raw.contains("sonnet") { return "Sonnet" }
        if raw.contains("haiku") { return "Haiku" }
        return raw
    }
}
