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
            if needsPermission, let t = transcript, t.status == .working {
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
            case .compacting:
                return .compacting
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
