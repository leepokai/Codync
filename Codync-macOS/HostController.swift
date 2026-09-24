import CodyncKit
import CodyncUI
import Foundation
import Observation
import ServiceManagement

/// Manages the local codync-host: finds the binary, installs the background
/// service, and owns the BotStore the menu and the chat window share.
@MainActor
@Observable
final class HostController {
    enum State: Equatable {
        case missingBinary
        case notInstalled
        case starting
        case running
        case failed(String)
    }

    /// `CODYNC_PORT` / `CODYNC_HOME` point the app at a dev host started with `codync-host serve`.
    static let devPort = ProcessInfo.processInfo.environment["CODYNC_PORT"].flatMap(Int.init)
    static let port = devPort ?? 19222
    static let dataDir = ProcessInfo.processInfo.environment["CODYNC_HOME"].map { URL(filePath: $0) }
        ?? FileManager.default.homeDirectoryForCurrentUser.appending(path: ".codync")

    private(set) var state: State = .starting
    /// Live mirror of the host (same store the iPhone uses), once it's reachable.
    private(set) var store: BotStore?
    private(set) var pairInfo: PairingInfo?
    private(set) var version: String?
    var launchAtLogin: Bool = SMAppService.mainApp.status == .enabled

    private var streamTask: Task<Void, Never>?

    var bots: [Bot] { store?.roster ?? [] }
    var usage: Usage { store?.usage ?? Usage() }
    var needsAttention: Bool { bots.contains(where: \.needsInput) }
    var working: Int { bots.filter(\.isWorking).count }

    /// Bundled next to the app executable, else a Homebrew / cargo install.
    var binaryURL: URL? {
        let candidates = [
            Bundle.main.bundleURL.appending(path: "Contents/MacOS/codync-host"),
            URL(filePath: "/opt/homebrew/bin/codync-host"),
            URL(filePath: "/usr/local/bin/codync-host"),
            FileManager.default.homeDirectoryForCurrentUser.appending(path: ".cargo/bin/codync-host"),
        ]
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0.path) }
    }

    private var plistURL: URL {
        FileManager.default.homeDirectoryForCurrentUser.appending(path: "Library/LaunchAgents/com.pokai.codync.host.plist")
    }

    private var tokenURL: URL { Self.dataDir.appending(path: "token") }

    var logURL: URL { Self.dataDir.appending(path: "host.log") }

    func start() {
        refresh()
    }

    func refresh() {
        guard binaryURL != nil else {
            state = .missingBinary
            return
        }
        guard Self.devPort != nil || FileManager.default.fileExists(atPath: plistURL.path) else {
            state = .notInstalled
            return
        }
        connect()
    }

    /// `codync-host install` also removes Codync 1.x Claude hooks.
    func install() {
        guard let bin = binaryURL else { return }
        state = .starting
        Task {
            let result = await Self.run(bin, ["install", "--port", "\(Self.port)"])
            if result.status != 0 {
                state = .failed(result.output.isEmpty ? "Install failed" : result.output)
            } else {
                try? await Task.sleep(for: .seconds(1))
                connect()
            }
        }
    }

    func restart() {
        let uid = getuid()
        Task {
            _ = await Self.run(URL(filePath: "/bin/launchctl"), ["kickstart", "-k", "gui/\(uid)/com.pokai.codync.host"])
            try? await Task.sleep(for: .seconds(1))
            connect()
        }
    }

    func uninstall() {
        guard let bin = binaryURL else { return }
        streamTask?.cancel()
        Task {
            _ = await Self.run(bin, ["uninstall"])
            store = nil
            state = .notInstalled
        }
    }

    func loadPairing() {
        guard let client = client() else { return }
        Task { pairInfo = try? await client.pairing() }
    }

    func setLaunchAtLogin(_ on: Bool) {
        do {
            if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }
        } catch {}
        launchAtLogin = SMAppService.mainApp.status == .enabled
    }

    func stop(_ bot: Bot) {
        Task { try? await client()?.stop(bot.id) }
    }

    private func client() -> HostClient? {
        guard let token = try? String(contentsOf: tokenURL, encoding: .utf8) else { return nil }
        return HostClient(baseURL: URL(string: "http://127.0.0.1:\(Self.port)")!, token: token.trimmingCharacters(in: .whitespacesAndNewlines))
    }

    /// Watches the host's health; the BotStore handles the event stream itself.
    private func connect() {
        streamTask?.cancel()
        streamTask = Task { [weak self] in
            var failures = 0
            while !Task.isCancelled {
                guard let self else { return }
                if let client = self.client(), await client.healthy(timeout: 2) {
                    failures = 0
                    if self.store == nil || self.store?.pairing?.token != client.token {
                        self.version = (try? await client.hello())?.version
                        let store = BotStore(
                            pairing: Pairing(name: Host.current().localizedName ?? "This Mac", token: client.token, urls: [client.baseURL.absoluteString]),
                            clientKind: "mac",
                            persistsPairing: false
                        )
                        store.restartStream()
                        self.store = store
                    }
                    self.state = .running
                } else {
                    failures += 1
                    if failures > 5 { self.state = .failed("The host isn't responding. See the log for details.") }
                    else if self.state != .running { self.state = .starting }
                }
                try? await Task.sleep(for: .seconds(5))
            }
        }
    }

    nonisolated static func run(_ bin: URL, _ args: [String]) async -> (status: Int32, output: String) {
        await withCheckedContinuation { cont in
            let p = Process()
            p.executableURL = bin
            p.arguments = args
            // The host rebuilds PATH from the login shell itself (backends::hydrate_path).
            let pipe = Pipe()
            p.standardOutput = pipe
            p.standardError = pipe
            p.terminationHandler = { proc in
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                cont.resume(returning: (proc.terminationStatus, String(decoding: data, as: UTF8.self)))
            }
            do { try p.run() } catch { cont.resume(returning: (-1, error.localizedDescription)) }
        }
    }
}
