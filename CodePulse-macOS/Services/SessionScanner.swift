import Foundation
import Combine
import os

private let logger = Logger(subsystem: "com.pokai.CodePulse", category: "SessionScanner")

@MainActor
final class SessionScanner: ObservableObject {
    @Published var activeSessions: [String: RawSessionFile] = [:]

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
        logger.info("SessionScanner stopped")
    }

    private func scheduleScan() {
        debounceTask?.cancel()
        debounceTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(debounceInterval))
            guard !Task.isCancelled else { return }
            scan()
        }
    }

    func scan() {
        let files = SessionFileParser.parseSessionFiles()
        var newSessions: [String: RawSessionFile] = [:]
        for file in files {
            guard PIDChecker.isAlive(pid: file.pid) else { continue }
            newSessions[file.sessionId] = file
        }
        if newSessions.count != activeSessions.count {
            logger.info("Active sessions: \(newSessions.count)")
        }
        activeSessions = newSessions
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
