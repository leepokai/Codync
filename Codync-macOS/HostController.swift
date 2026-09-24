import CodyncKit
import Foundation
import Observation
import ServiceManagement

/// Manages the local codync-host: finds the binary, installs the background
/// service, and mirrors bots + usage over the same API the phone uses.
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

    struct PairInfo: Decodable {
        var urls: [String]
        var pairingUrl: String
    }

    /// `CODYNC_PORT` / `CODYNC_HOME` point the app at a dev host started with `codync-host serve`.
    static let devPort = ProcessInfo.processInfo.environment["CODYNC_PORT"].flatMap(Int.init)
    static let port = devPort ?? 19222
    static let dataDir = ProcessInfo.processInfo.environment["CODYNC_HOME"].map { URL(filePath: $0) }
        ?? FileManager.default.homeDirectoryForCurrentUser.appending(path: ".codync")

    private(set) var state: State = .starting
    private(set) var bots: [Bot] = []
    private(set) var usage = Usage()
    private(set) var pairInfo: PairInfo?
    private(set) var version: String?
    var launchAtLogin: Bool = SMAppService.mainApp.status == .enabled

    private var streamTask: Task<Void, Never>?

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
            bots = []
            state = .notInstalled
        }
    }

    func loadPairing() {
        guard let bin = binaryURL else { return }
        Task {
            let result = await Self.run(bin, ["pair", "--json", "--port", "\(Self.port)"])
            pairInfo = try? JSONDecoder().decode(PairInfo.self, from: Data(result.output.utf8))
        }
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

    private func connect() {
        streamTask?.cancel()
        streamTask = Task { [weak self] in
            var attempts = 0
            while !Task.isCancelled {
                guard let self else { return }
                guard let client = self.client(), await client.healthy(timeout: 2) else {
                    attempts += 1
                    self.state = attempts > 5 ? .failed("The host isn't responding. See the log for details.") : .starting
                    try? await Task.sleep(for: .seconds(2))
                    continue
                }
                attempts = 0
                self.version = (try? await client.hello())?.version
                var mirror: [String: Bot] = [:]
                do {
                    for try await event in client.events(since: 0, client: "mac") {
                        self.state = .running
                        switch event {
                        case let .bot(bot): mirror[bot.id] = bot
                        case let .botDeleted(id, _): mirror[id] = nil
                        case let .hello(_, _, usage), let .usage(usage): self.usage = usage
                        case .entry, .resync, .undecodable: break
                        }
                        self.bots = mirror.values.filter { !$0.hidden }.sorted { $0.lastAt > $1.lastAt }
                    }
                } catch {}
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }

    nonisolated static func run(_ bin: URL, _ args: [String]) async -> (status: Int32, output: String) {
        await withCheckedContinuation { cont in
            let p = Process()
            p.executableURL = bin
            p.arguments = args
            // GUI apps get a minimal PATH; give the service the usual tool locations.
            var env = ProcessInfo.processInfo.environment
            let home = FileManager.default.homeDirectoryForCurrentUser.path
            let extra = ["/opt/homebrew/bin", "/usr/local/bin", "\(home)/.local/bin", "\(home)/.cargo/bin", "\(home)/.bun/bin", "\(home)/.opencode/bin"]
            env["PATH"] = (extra + [loginShellPath() ?? "/usr/bin:/bin:/usr/sbin:/sbin"]).joined(separator: ":")
            p.environment = env
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

    /// The user's interactive PATH (nvm, asdf, …) so npx/claude/codex resolve in the service.
    nonisolated static func loginShellPath() -> String? {
        let shell = ProcessInfo.processInfo.environment["SHELL"] ?? "/bin/zsh"
        let p = Process()
        p.executableURL = URL(filePath: shell)
        p.arguments = ["-ilc", "printf %s \"$PATH\""]
        let pipe = Pipe()
        p.standardOutput = pipe
        p.standardError = FileHandle.nullDevice
        guard (try? p.run()) != nil else { return nil }
        p.waitUntilExit()
        let out = String(decoding: pipe.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
        return out.isEmpty ? nil : out
    }
}
