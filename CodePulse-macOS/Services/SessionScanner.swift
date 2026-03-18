import Foundation
import Combine
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "SessionScanner")

@MainActor
final class SessionScanner: ObservableObject {
    @Published var activeSessions: [String: RawSessionFile] = [:]

    /// Reference to the hook server for cwd→sessionId resolution
    var hookServer: ClaudeHookServer?

    private var sessionsWatcher: DispatchSourceFileSystemObject?
    private var scanTimer: Timer?
    private let debounceInterval: TimeInterval = 1.0
    private var debounceTask: Task<Void, Never>?

    func start() {
        scan()
        watchDirectory(ClaudePaths.sessionsDir) { [weak self] in
            self?.scheduleScan()
        }
        scanTimer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.scan()
            }
        }
        logger.info("SessionScanner started, watching \(ClaudePaths.sessionsDir.path)")
    }

    func stop() {
        sessionsWatcher?.cancel()
        sessionsWatcher = nil
        scanTimer?.invalidate()
        scanTimer = nil
        debounceTask?.cancel()
    }

    private func scheduleScan() {
        debounceTask?.cancel()
        debounceTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(debounceInterval))
            guard !Task.isCancelled else { return }
            scan()
        }
    }

    /// Called from hook events — uses the same debounce to avoid redundant scans.
    func scheduleScanFromHook() {
        scheduleScan()
    }

    func scan() {
        let files = SessionFileParser.parseSessionFiles()
        var newSessions: [String: RawSessionFile] = [:]

        for file in files {
            guard PIDChecker.isAlive(pid: file.pid) else { continue }

            // Resolution priority:
            // 1. Hook server knows the real sessionId (from hook events with cwd)
            // 2. Fall back to JSONL directory scan (single-session projects only)
            // 3. Use the session file's original sessionId
            let resolved = resolveSessionId(file)
            newSessions[resolved.sessionId] = resolved
        }

        if newSessions != activeSessions {
            if newSessions.count != activeSessions.count {
                logger.info("Active sessions: \(newSessions.count)")
            }
            activeSessions = newSessions
        }
    }

    /// Resolve the real sessionId for a session file.
    /// Hook events provide the authoritative mapping (cwd → sessionId).
    /// Falls back to JSONL directory scan when no hook data is available.
    private func resolveSessionId(_ file: RawSessionFile) -> RawSessionFile {
        // 1. Ask hook server — it has the real sessionId from live events
        if let hookSessionId = hookServer?.activeSessionId(forCwd: file.cwd),
           hookSessionId != file.sessionId {
            return RawSessionFile(pid: file.pid, sessionId: hookSessionId, cwd: file.cwd, startedAt: file.startedAt)
        }

        // 2. Check if original JSONL exists — if so, no resolution needed
        let originalJsonl = ClaudePaths.jsonlPath(cwd: file.cwd, sessionId: file.sessionId)
        if FileManager.default.fileExists(atPath: originalJsonl.path) {
            return file
        }

        // 3. Fall back to finding the most recently modified JSONL in the project directory
        return resolveFromJsonlDirectory(file)
    }

    /// Last-resort: scan the project directory for the most recently modified JSONL.
    /// Used when no hook data is available and the original JSONL doesn't exist.
    private func resolveFromJsonlDirectory(_ file: RawSessionFile) -> RawSessionFile {
        let projectDir = ClaudePaths.projectsDir
            .appendingPathComponent(ClaudePaths.mangledCwd(file.cwd))

        guard let contents = try? FileManager.default.contentsOfDirectory(
            at: projectDir, includingPropertiesForKeys: [.contentModificationDateKey]
        ) else {
            return file
        }

        let jsonlFiles = contents.filter { url in
            url.pathExtension == "jsonl"
            && !url.lastPathComponent.hasPrefix("agent-")
        }

        var bestURL: URL?
        var bestDate: Date = .distantPast
        for url in jsonlFiles {
            guard let values = try? url.resourceValues(forKeys: [.contentModificationDateKey]),
                  let modDate = values.contentModificationDate else { continue }
            if modDate > bestDate {
                bestDate = modDate
                bestURL = url
            }
        }

        guard let activeURL = bestURL else { return file }
        let activeSessionId = activeURL.deletingPathExtension().lastPathComponent

        // Only resolve if the best JSONL was modified recently (within 2 minutes)
        let age = Date().timeIntervalSince(bestDate)
        guard age < 120 else { return file }

        if activeSessionId != file.sessionId {
            logger.info("Session PID=\(file.pid): resolved \(file.sessionId.prefix(8))→\(activeSessionId.prefix(8)) (active JSONL)")
            return RawSessionFile(
                pid: file.pid,
                sessionId: activeSessionId,
                cwd: file.cwd,
                startedAt: file.startedAt
            )
        }

        return file
    }

    private func watchDirectory(_ url: URL, handler: @escaping () -> Void) {
        let fd = open(url.path, O_EVTONLY)
        guard fd >= 0 else {
            logger.warning("Failed to watch directory: \(url.path)")
            return
        }
        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .rename, .delete],
            queue: .main
        )
        source.setEventHandler(handler: handler)
        source.setCancelHandler { close(fd) }
        source.resume()
        sessionsWatcher = source
    }
}
