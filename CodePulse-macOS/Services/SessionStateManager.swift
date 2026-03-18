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

    /// Force a status re-evaluation.
    /// Called when hook state changes (Stop, Permission, etc.).
    /// Re-scans first so sessionIds are resolved with latest hook data,
    /// then runs updateSessions even if the session list looks the same.
    func refreshFromHookState() {
        scanner.scan()
        updateSessions(from: scanner.activeSessions)
    }

    private func updateSessions(from rawSessions: [String: RawSessionFile]) {
        var updated: [SessionState] = []

        for (sessionId, raw) in rawSessions {
            // Parse JSONL incrementally — supplementary data (model, tokens, cost, lastEvent)
            let jsonlUrl = ClaudePaths.jsonlPath(cwd: raw.cwd, sessionId: sessionId)
            transcriptWatcher.update(sessionId: sessionId, jsonlURL: jsonlUrl)

            let transcript = transcriptWatcher.state(for: sessionId)
            let tasks = SessionFileParser.parseTasks(sessionId: sessionId)
            // Try exact sessionId first, then fall back to latest entry for this project
            // (handles resolved sessions where sessionId differs from index)
            let indexEntry = SessionFileParser.parseSessionIndex(cwd: raw.cwd, sessionId: sessionId)
                ?? SessionFileParser.parseLatestSessionIndex(cwd: raw.cwd)

            // Hook-driven: clear stale hook states when transcript shows activity resumed
            let needsPermission = hookServer?.needsPermission(sessionId) ?? false
            if needsPermission, let t = transcript, t.status == .working {
                hookServer?.clearPermission(sessionId)
            }
            if hookServer?.isStopped(sessionId) == true, let t = transcript, t.status == .working {
                hookServer?.clearStop(sessionId)
            }

            let (status, waitingReason) = detectStatus(
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

            // Summary priority: sessions-index summary > lastPrompt > firstPrompt > project name
            // Filter out system messages like "[Request interrupted by user]"
            let isUsable = { (s: String) -> Bool in
                !s.isEmpty && !s.hasPrefix("[Request") && !s.hasPrefix("[Error")
            }
            let summary: String = {
                if let s = indexEntry?.summary, isUsable(s) { return s }
                if let lp = transcript?.lastPrompt, isUsable(lp) { return lp }
                if let fp = transcript?.firstPrompt, isUsable(fp) { return fp }
                if let fp = indexEntry?.firstPrompt, isUsable(fp) { return String(fp.prefix(80)) }
                return projectName
            }()

            // Git branch: sessions-index > direct git query > "unknown"
            let gitBranch = indexEntry?.gitBranch ?? Self.gitBranch(cwd: raw.cwd)

            let existingSession = sessions.first { $0.sessionId == sessionId }
            let updatedAt = transcript?.lastActivityTime ?? existingSession?.updatedAt ?? startDate

            let session = SessionState(
                sessionId: sessionId,
                projectName: projectName,
                gitBranch: gitBranch,
                status: status,
                model: formatModel(transcript?.model ?? "Unknown"),
                summary: summary,
                currentTask: currentTask,
                lastEvent: lastEvent,
                waitingReason: waitingReason,
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
                completed.waitingReason = nil
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

    private var skipPermissionsCache: [Int: Bool] = [:]

    private func isSkipPermissions(pid: Int) -> Bool {
        if let cached = skipPermissionsCache[pid] { return cached }
        let result = PIDChecker.skipsPermissions(pid: pid)
        skipPermissionsCache[pid] = result
        return result
    }

    /// Hook-driven status detection.
    /// Priority: PID check → Stop hook → Permission hook → Tool running hook → Transcript → Tasks → Idle
    private func detectStatus(
        raw: RawSessionFile,
        transcript: TranscriptState?,
        tasks: [TaskItem],
        needsPermission: Bool
    ) -> (SessionStatus, WaitingReason?) {
        // 1. PID dead → completed
        guard PIDChecker.isAlive(pid: raw.pid) else {
            skipPermissionsCache.removeValue(forKey: raw.pid)
            return (.completed, nil)
        }

        let bypassed = isSkipPermissions(pid: raw.pid)
        let sessionId = raw.sessionId

        // 2. Hook: Stop fired → waiting for user input (yellow)
        if hookServer?.isStopped(sessionId) == true {
            return (.needsInput, .commandComplete)
        }

        // 3. Hook: Permission/elicitation/askUserQuestion needed (red or yellow)
        if needsPermission && !bypassed {
            let reason: WaitingReason
            if let lastEvent = transcript?.lastEvent {
                if lastEvent.contains("Waiting for input") {
                    reason = .askUserQuestion
                } else {
                    reason = .permissionPrompt
                }
            } else {
                reason = .permissionPrompt
            }
            return (.needsInput, reason)
        }

        // 4. Hook: Tool currently running → working
        if hookServer?.isToolRunning(sessionId) == true {
            return (.working, nil)
        }

        // 5. Transcript state (supplementary — for working/compacting/idle)
        if let transcript {
            switch transcript.status {
            case .working:
                return (.working, nil)
            case .waitingForUser:
                return (bypassed ? .idle : .needsInput, bypassed ? nil : .unknown)
            case .compacting:
                return (.compacting, nil)
            case .idle:
                return (.idle, nil)
            }
        }

        // 6. Fallback: check tasks
        if tasks.contains(where: { $0.status == .inProgress }) { return (.working, nil) }

        // 7. No signal → idle
        return (.idle, nil)
    }

    private static var gitBranchCache: [String: (branch: String, time: Date)] = [:]

    private static func gitBranch(cwd: String) -> String {
        // Cache for 30 seconds to avoid spawning git on every scan
        if let cached = gitBranchCache[cwd],
           Date().timeIntervalSince(cached.time) < 30 {
            return cached.branch
        }
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        proc.arguments = ["-C", cwd, "rev-parse", "--abbrev-ref", "HEAD"]
        proc.currentDirectoryURL = URL(fileURLWithPath: cwd)
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = FileHandle.nullDevice
        do {
            try proc.run()
            proc.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let branch = String(data: data, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            let result = branch.isEmpty ? "unknown" : branch
            gitBranchCache[cwd] = (result, Date())
            return result
        } catch {
            return "unknown"
        }
    }

    private func formatModel(_ raw: String) -> String {
        if raw.contains("opus") { return "Opus" }
        if raw.contains("sonnet") { return "Sonnet" }
        if raw.contains("haiku") { return "Haiku" }
        if raw == "Unknown" || raw.contains("synthetic") { return "" }
        return raw
    }
}
