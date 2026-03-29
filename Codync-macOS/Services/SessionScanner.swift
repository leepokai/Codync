import Foundation
import Combine
import os

private let logger = Logger(subsystem: "com.pokai.Codync", category: "SessionScanner")

@MainActor
final class SessionScanner: ObservableObject {
    @Published var activeSessions: [String: RawSessionFile] = [:]

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
        logger.info("SessionScanner started")
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

    func scheduleScanFromHook() {
        scheduleScan()
    }

    func scan() {
        let files = SessionFileParser.parseSessionFiles()
        var newSessions: [String: RawSessionFile] = [:]

        var alivePids = Set<Int>()
        for file in files {
            guard PIDChecker.isAlive(pid: file.pid) else { continue }
            alivePids.insert(file.pid)
            let resolved = resolveSessionId(file)
            // Dict keyed by sessionId — if two PIDs resolve to the same JSONL,
            // the later one wins (acceptable: they share the same transcript)
            newSessions[resolved.sessionId] = resolved
        }

        // Clean up TTY cache for dead PIDs
        ttyCache = ttyCache.filter { alivePids.contains($0.key) }

        if newSessions != activeSessions {
            if newSessions.count != activeSessions.count {
                logger.info("Active sessions: \(newSessions.count)")
            }
            activeSessions = newSessions
        }
    }

    // MARK: - Session ID Resolution

    private func resolveSessionId(_ file: RawSessionFile) -> RawSessionFile {
        // 1. TTY-based resolution (most precise — each terminal has a unique TTY)
        if let tty = detectAndCacheTty(pid: file.pid),
           let hookSessionId = hookServer?.sessionId(forTty: tty) {
            return RawSessionFile(pid: file.pid, sessionId: hookSessionId, cwd: file.cwd, startedAt: file.startedAt)
        }

        // 2. Original JSONL exists — but verify it's the most recent one.
        //    Claude Code can keep stale PID files pointing to old sessionIds
        //    while actively writing to a newer JSONL.
        let originalJsonl = ClaudePaths.jsonlPath(cwd: file.cwd, sessionId: file.sessionId)
        if FileManager.default.fileExists(atPath: originalJsonl.path) {
            let resolved = resolveFromJsonlDirectory(file)
            if resolved.sessionId != file.sessionId {
                // A more recently modified JSONL exists — use it instead
                return resolved
            }
            return file
        }

        // 3. Hook server cwd-based (when only 1 session per cwd, unambiguous)
        let hookIds = hookServer?.activeSessionIds(forCwd: file.cwd) ?? []
        if hookIds.count == 1, let hookId = hookIds.first {
            let hookJsonl = ClaudePaths.jsonlPath(cwd: file.cwd, sessionId: hookId)
            if FileManager.default.fileExists(atPath: hookJsonl.path) {
                return RawSessionFile(pid: file.pid, sessionId: hookId, cwd: file.cwd, startedAt: file.startedAt)
            }
        }

        // 4. Last resort: most recently modified JSONL in the project directory
        return resolveFromJsonlDirectory(file)
    }

    // MARK: - TTY Cache (caches both hits and misses to avoid repeated subprocess spawns)

    private var ttyCache: [Int: String?] = [:]

    private func detectAndCacheTty(pid: Int) -> String? {
        if let cached = ttyCache[pid] { return cached }
        let tty = PIDChecker.tty(for: pid)
        ttyCache[pid] = tty  // cache nil too — avoids retrying every 5s
        return tty
    }

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

        if activeSessionId != file.sessionId {
            return RawSessionFile(pid: file.pid, sessionId: activeSessionId, cwd: file.cwd, startedAt: file.startedAt)
        }

        return file
    }

    // MARK: - Directory Watching

    private func watchDirectory(_ url: URL, handler: @escaping () -> Void) {
        let fd = open(url.path, O_EVTONLY)
        guard fd >= 0 else { return }
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
