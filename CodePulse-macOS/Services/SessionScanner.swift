import Foundation
import Combine

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

    func scan() {
        let files = SessionFileParser.parseSessionFiles()
        var newSessions: [String: RawSessionFile] = [:]
        for file in files {
            guard PIDChecker.isAlive(pid: file.pid) else { continue }
            newSessions[file.sessionId] = file
        }
        activeSessions = newSessions
    }

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
