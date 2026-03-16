import Foundation
import Combine
import CodePulseShared

@MainActor
final class SessionStateManager: ObservableObject {
    @Published var sessions: [SessionState] = []

    private let scanner: SessionScanner
    private var cancellables = Set<AnyCancellable>()
    private var jsonlSizes: [String: UInt64] = [:]
    private var jsonlSizeTimestamps: [String: Date] = [:]
    private let deviceId = Host.current().localizedName ?? UUID().uuidString

    init(scanner: SessionScanner) {
        self.scanner = scanner
        scanner.$activeSessions
            .debounce(for: .seconds(1), scheduler: RunLoop.main)
            .sink { [weak self] rawSessions in
                self?.updateSessions(from: rawSessions)
            }
            .store(in: &cancellables)
    }

    private func updateSessions(from rawSessions: [String: RawSessionFile]) {
        var updated: [SessionState] = []

        for (sessionId, raw) in rawSessions {
            let tasks = SessionFileParser.parseTasks(sessionId: sessionId)
            let indexEntry = SessionFileParser.parseSessionIndex(cwd: raw.cwd, sessionId: sessionId)
            let jsonlUrl = ClaudePaths.jsonlPath(cwd: raw.cwd, sessionId: sessionId)
            let jsonlInfo = JSONLTailReader.extractInfo(url: jsonlUrl)
            let status = detectStatus(raw: raw, tasks: tasks, jsonlUrl: jsonlUrl)

            let projectName = URL(fileURLWithPath: raw.cwd).lastPathComponent
            let startDate = Date(timeIntervalSince1970: TimeInterval(raw.startedAt) / 1000)
            let duration = Int(Date().timeIntervalSince(startDate))
            let currentTask = tasks.first(where: { $0.status == .inProgress })?.activeForm

            let summary = indexEntry?.summary
                ?? indexEntry?.firstPrompt.map { String($0.prefix(50)) }
                ?? projectName

            // Only update timestamp if content actually changed
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

        sessions = updated.sorted { $0.startedAt > $1.startedAt }
    }

    private func detectStatus(raw: RawSessionFile, tasks: [TaskItem], jsonlUrl: URL) -> SessionStatus {
        guard PIDChecker.isAlive(pid: raw.pid) else { return .completed }
        if tasks.contains(where: { $0.status == .inProgress }) { return .working }

        let currentSize = JSONLTailReader.fileSize(url: jsonlUrl) ?? 0
        let previousSize = jsonlSizes[raw.sessionId] ?? currentSize
        let lastChange = jsonlSizeTimestamps[raw.sessionId] ?? Date()

        if currentSize != previousSize {
            jsonlSizes[raw.sessionId] = currentSize
            jsonlSizeTimestamps[raw.sessionId] = Date()
            return .working
        }

        if Date().timeIntervalSince(lastChange) > 30 {
            return .needsInput
        }

        return .idle
    }

    private func formatModel(_ raw: String) -> String {
        if raw.contains("opus") { return "Opus" }
        if raw.contains("sonnet") { return "Sonnet" }
        if raw.contains("haiku") { return "Haiku" }
        return raw
    }
}
