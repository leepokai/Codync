import CodyncKit
import Darwin
import Foundation
import Observation
import OSLog

private let log = Logger(subsystem: "com.pokai.Codync", category: "ssh")

/// A computer reached over SSH (spec §11.4). Nothing secret: keys stay in ssh-agent or on disk.
struct SSHProfile: Codable, Hashable, Identifiable, Sendable {
    var id = UUID()
    /// An `~/.ssh/config` alias or a hostname.
    var host: String
    var port: Int?
    var user: String?
    var identityFile: String?
    var remotePort = 19222
    /// Remembered after the first connection; a different computer behind the same address is refused.
    var computerId: ComputerID?
    var name: String
}

/// Everything that turns a profile into OpenSSH argv, and reads OpenSSH's output back.
/// Pure functions: arguments are always an argv array, never a shell string.
enum SSH {
    static let ssh = URL(filePath: "/usr/bin/ssh")
    static let keygen = URL(filePath: "/usr/bin/ssh-keygen")
    static let keyscan = URL(filePath: "/usr/bin/ssh-keyscan")
    static let lsof = URL(filePath: "/usr/sbin/lsof")
    static let installCommand = "brew install leepokai/codync/codync-host && codync-host install"

    static var home: String { FileManager.default.homeDirectoryForCurrentUser.path }

    /// A reason the profile can't be used, or nil.
    static func validate(_ p: SSHProfile) -> String? {
        if p.host.hasPrefix("-") || !matches(p.host, #"^[A-Za-z0-9._-]+$"#) {
            return "Use an SSH alias or a hostname: letters, digits, dots, dashes and underscores."
        }
        if let user = p.user, !matches(user, #"^[A-Za-z_][A-Za-z0-9._-]*$"#) { return "That user name isn't valid." }
        if let port = p.port, !(1...65535).contains(port) { return "The SSH port must be between 1 and 65535." }
        if !(1...65535).contains(p.remotePort) { return "The Codync port must be between 1 and 65535." }
        if let file = p.identityFile {
            var isDir: ObjCBool = false
            guard file.hasPrefix("/"), FileManager.default.fileExists(atPath: file, isDirectory: &isDir), !isDir.boolValue else {
                return "The key file doesn't exist."
            }
        }
        return nil
    }

    static func knownHostsFiles(home: String) -> [String] {
        ["\(home)/.ssh/known_hosts", "\(home)/.codync/ssh_known_hosts"]
    }

    /// ssh's config tokenizer splits `UserKnownHostsFile` on spaces, so each path carries literal quotes.
    static func hostKeyOptions(home: String) -> [String] {
        ["-o", "UserKnownHostsFile=" + knownHostsFiles(home: home).map { "\"\($0)\"" }.joined(separator: " "),
         "-o", "StrictHostKeyChecking=yes"]
    }

    static func targetOptions(_ p: SSHProfile) -> [String] {
        var args: [String] = []
        if let port = p.port { args += ["-p", "\(port)"] }
        if let key = p.identityFile { args += ["-i", key] }
        if let user = p.user { args += ["-l", user] }
        return args
    }

    static func resolveArguments(_ p: SSHProfile) -> [String] {
        var args = ["-G"]
        if let port = p.port { args += ["-p", "\(port)"] }
        if let user = p.user { args += ["-l", user] }
        return args + ["--", p.host]
    }

    /// The remote command is fixed; only the validated port number goes into it.
    static func infoArguments(_ p: SSHProfile, home: String) -> [String] {
        ["-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "ForwardAgent=no", "-o", "ForwardX11=no"]
            + hostKeyOptions(home: home) + targetOptions(p)
            + ["--", p.host, "sh -lc 'codync-host info --json --port \(p.remotePort)'"]
    }

    static func tunnelArguments(_ p: SSHProfile, localPort: Int, home: String) -> [String] {
        ["-N", "-T", "-o", "ExitOnForwardFailure=yes", "-o", "ServerAliveInterval=15", "-o", "ServerAliveCountMax=3",
         "-o", "ForwardAgent=no", "-o", "ForwardX11=no", "-o", "BatchMode=yes"]
            + hostKeyOptions(home: home)
            + ["-L", "127.0.0.1:\(localPort):127.0.0.1:\(p.remotePort)"]
            + targetOptions(p) + ["--", p.host]
    }

    /// What `ssh -G` resolved the destination to.
    struct Resolved: Equatable, Sendable {
        var hostname: String
        var port: Int
        var hostKeyAlias: String?
        var proxyJump: String?
        var proxyCommand: String?

        var usesProxy: Bool { proxyJump != nil || proxyCommand != nil }
        /// The name ssh looks up in known_hosts.
        var knownHostsName: String {
            let name = hostKeyAlias ?? hostname
            return port == 22 ? name : "[\(name)]:\(port)"
        }
    }

    static func parseConfig(_ text: String) -> Resolved? {
        var values: [String: String] = [:]
        for line in text.split(whereSeparator: \.isNewline) {
            let parts = line.split(separator: " ", maxSplits: 1)
            guard parts.count == 2 else { continue }
            let key = parts[0].lowercased()
            if values[key] == nil { values[key] = String(parts[1]).trimmingCharacters(in: .whitespaces) }
        }
        func set(_ key: String) -> String? { values[key].flatMap { $0.isEmpty || $0 == "none" ? nil : $0 } }
        guard let hostname = set("hostname"), let port = values["port"].flatMap(Int.init) else { return nil }
        return Resolved(hostname: hostname, port: port, hostKeyAlias: set("hostkeyalias"),
                        proxyJump: set("proxyjump"), proxyCommand: set("proxycommand"))
    }

    static func keyscanArguments(_ r: Resolved) -> [String] {
        ["-T", "5", "-p", "\(r.port)", "--", r.hostname]
    }

    /// `type key` of each host key line (known_hosts or ssh-keyscan output); comments and revoked keys skipped.
    static func hostKeys(_ text: String) -> Set<String> {
        Set(text.split(whereSeparator: \.isNewline).compactMap { line in
            var fields = line.split(whereSeparator: \.isWhitespace)
            guard let first = fields.first, !first.hasPrefix("#") else { return nil }
            if first.hasPrefix("@") {
                guard first == "@cert-authority" else { return nil }
                fields.removeFirst()
            }
            return fields.count >= 3 ? "\(fields[1]) \(fields[2])" : nil
        })
    }

    /// ssh-keyscan's lines, filed under the name ssh will look up.
    static func knownHostsLines(scan: String, name: String) -> [String] {
        hostKeys(scan).sorted().map { "\(name) \($0)" }
    }

    private static func matches(_ s: String, _ pattern: String) -> Bool {
        s.range(of: pattern, options: .regularExpression) != nil
    }

    #if DEBUG
    /// Runs at launch in debug builds (spec §11.5).
    static func selfCheck() {
        let home = "/Users/k"
        let p = SSHProfile(host: "box", port: 2222, user: "kevin", name: "Box")
        assert(hostKeyOptions(home: home) == [
            "-o", #"UserKnownHostsFile="/Users/k/.ssh/known_hosts" "/Users/k/.codync/ssh_known_hosts""#,
            "-o", "StrictHostKeyChecking=yes",
        ])
        assert(resolveArguments(p) == ["-G", "-p", "2222", "-l", "kevin", "--", "box"])
        assert(Array(tunnelArguments(p, localPort: 50000, home: home).suffix(8))
            == ["-L", "127.0.0.1:50000:127.0.0.1:19222", "-p", "2222", "-l", "kevin", "--", "box"])
        assert(infoArguments(p, home: home).last == "sh -lc 'codync-host info --json --port 19222'")
        assert(tunnelArguments(p, localPort: 1, home: home).contains("ExitOnForwardFailure=yes"))
        assert(validate(p) == nil)
        assert(validate(SSHProfile(host: "-oProxyCommand=x", name: "")) != nil)
        assert(validate(SSHProfile(host: "a b", name: "")) != nil)
        assert(validate(SSHProfile(host: "box;rm", name: "")) != nil)
        assert(validate(SSHProfile(host: "box", user: "1x", name: "")) != nil)
        assert(validate(SSHProfile(host: "box", port: 0, name: "")) != nil)
        assert(validate(SSHProfile(host: "box", identityFile: "/nonexistent/key", name: "")) != nil)
        let config = "user kevin\nhostname 10.0.0.5\nport 2222\nhostkeyalias devbox\nproxycommand none\n"
        let r = parseConfig(config)
        assert(r == Resolved(hostname: "10.0.0.5", port: 2222, hostKeyAlias: "devbox", proxyJump: nil, proxyCommand: nil))
        assert(r?.knownHostsName == "[devbox]:2222")
        assert(parseConfig("hostname box\nport 22\nproxyjump gw\n")?.knownHostsName == "box")
        assert(parseConfig("hostname box\nport 22\nproxyjump gw\n")?.usesProxy == true)
        let scan = "# box:22 SSH-2.0\nbox ssh-ed25519 AAAA1\n[box]:22 ecdsa-sha2-nistp256 BBBB\n"
        assert(hostKeys(scan) == ["ssh-ed25519 AAAA1", "ecdsa-sha2-nistp256 BBBB"])
        assert(hostKeys("@revoked box ssh-ed25519 AAAA1\n").isEmpty)
        assert(knownHostsLines(scan: scan, name: "[devbox]:2222")
            == ["[devbox]:2222 ecdsa-sha2-nistp256 BBBB", "[devbox]:2222 ssh-ed25519 AAAA1"])
    }
    #endif
}

struct ProcessResult: Sendable {
    var status: Int32
    var stdout: Data
    var stderr: String
    var text: String { String(decoding: stdout, as: UTF8.self) }
    /// The last thing the tool said, for showing next to an error.
    var detail: String {
        stderr.split(whereSeparator: \.isNewline).last.map(String.init) ?? ""
    }
}

enum ProcessRunner {
    /// Runs a tool to completion; both pipes are drained while it runs.
    static func run(_ executable: URL, _ args: [String], input: Data? = nil) async -> ProcessResult {
        let process = Process()
        process.executableURL = executable
        process.arguments = args
        let out = Pipe()
        let err = Pipe()
        let inPipe = Pipe()
        process.standardOutput = out
        process.standardError = err
        process.standardInput = input == nil ? FileHandle.nullDevice : inPipe
        let outHandle = out.fileHandleForReading
        let errHandle = err.fileHandleForReading
        async let stdout = Task.detached { outHandle.readDataToEndOfFile() }.value
        async let stderr = Task.detached { errHandle.readDataToEndOfFile() }.value
        let status: Int32 = await withCheckedContinuation { cont in
            process.terminationHandler = { cont.resume(returning: $0.terminationStatus) }
            do {
                try process.run()
            } catch {
                process.terminationHandler = nil
                try? out.fileHandleForWriting.close()
                try? err.fileHandleForWriting.close()
                cont.resume(returning: -1)
                return
            }
            if let input {
                inPipe.fileHandleForWriting.write(input)
                try? inPipe.fileHandleForWriting.close()
            }
        }
        return await ProcessResult(status: status, stdout: stdout, stderr: String(decoding: stderr, as: UTF8.self))
    }
}

/// What `codync-host info --json` prints.
private struct RemoteInfo: Decodable {
    var name: String
    var computerId: ComputerID
    var signKey: String
    var boxKey: String?
    var version: String?
    var token: String
    var running: Bool
}

/// `GET /health` on a loopback host.
struct HostHealth: Decodable {
    var hostId: String?
    var computerId: ComputerID?
    var version: String?

    static func fetch(_ baseURL: URL, timeout: TimeInterval = 2) async -> HostHealth? {
        var request = URLRequest(url: baseURL.appending(path: "health"))
        request.timeoutInterval = timeout
        guard let (data, response) = try? await URLSession.shared.data(for: request),
              (response as? HTTPURLResponse)?.statusCode == 200 else { return nil }
        return try? JSONDecoder().decode(HostHealth.self, from: data)
    }
}

/// SSH computers: profiles (UserDefaults), host key checks and one OpenSSH tunnel per connected profile.
/// Connected ones are handed to the account as loopback computers.
@MainActor
@Observable
final class SSHComputers {
    enum Status: Equatable {
        case idle
        case connecting(String)
        /// First time: the user compares these fingerprints before anything connects.
        case confirmHostKey(name: String, fingerprints: [String], lines: [String])
        case connected(localPort: Int)
        /// A retry is scheduled.
        case retrying(String)
        /// Needs the user (settings, the remote side, or a security problem); nothing retries.
        case failed(String)
        case notInstalled
    }

    struct Attachment {
        var computer: Computer
        var baseURL: URL
        var token: String
    }

    private static let defaultsKey = "sshProfiles"

    private(set) var profiles: [SSHProfile] = []
    private(set) var status: [UUID: Status] = [:]
    private(set) var attachments: [UUID: Attachment] = [:]

    var onAttach: ((Attachment) -> Void)?
    var onDetach: ((ComputerID) -> Void)?

    private var tunnels: [UUID: Process] = [:]
    private var tasks: [UUID: Task<Void, Never>] = [:]
    private var backoff: [UUID: Double] = [:]
    private var connectedAt: [UUID: Date] = [:]

    init() {
        profiles = UserDefaults.standard.data(forKey: Self.defaultsKey)
            .flatMap { try? JSONDecoder().decode([SSHProfile].self, from: $0) } ?? []
    }

    func status(of id: UUID) -> Status { status[id] ?? .idle }

    func connectAll() {
        for p in profiles where tasks[p.id] == nil && tunnels[p.id] == nil { connect(p.id) }
    }

    func add(_ profile: SSHProfile) {
        profiles.append(profile)
        persist()
        connect(profile.id)
    }

    func update(_ profile: SSHProfile) {
        guard let i = profiles.firstIndex(where: { $0.id == profile.id }) else { return }
        disconnect(profile.id)
        profiles[i] = profile
        persist()
        connect(profile.id)
    }

    func remove(_ id: UUID) {
        disconnect(id)
        profiles.removeAll { $0.id == id }
        status[id] = nil
        backoff[id] = nil
        persist()
    }

    func connect(_ id: UUID) {
        tasks[id]?.cancel()
        tasks[id] = Task { [weak self] in await self?.run(id) }
    }

    /// Ends the tunnel for this session (bots on it go away until the next connect).
    func disconnect(_ id: UUID) {
        tasks.removeValue(forKey: id)?.cancel()
        stopTunnel(id)
        status[id] = .idle
    }

    func disconnectAll() {
        for id in Array(tunnels.keys) + Array(tasks.keys) { disconnect(id) }
    }

    /// The user compared the fingerprints: remember the key in Codync's known_hosts and go on.
    func trustHostKey(_ id: UUID) {
        guard case let .confirmHostKey(_, _, lines) = status(of: id) else { return }
        do {
            let dir = URL(filePath: SSH.home).appending(path: ".codync")
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
            let file = dir.appending(path: "ssh_known_hosts")
            if !FileManager.default.fileExists(atPath: file.path) {
                FileManager.default.createFile(atPath: file.path, contents: nil, attributes: [.posixPermissions: 0o600])
            }
            let handle = try FileHandle(forWritingTo: file)
            defer { try? handle.close() }
            try handle.seekToEnd()
            try handle.write(contentsOf: Data((lines.joined(separator: "\n") + "\n").utf8))
        } catch {
            status[id] = .failed("Couldn't save the host key: \(error.localizedDescription)")
            return
        }
        connect(id)
    }

    // MARK: connecting

    private enum Outcome {
        case attached
        case stop(Status)
        case retry(String)
    }

    private func run(_ id: UUID) async {
        while !Task.isCancelled {
            guard let profile = profiles.first(where: { $0.id == id }) else { return }
            let outcome = await attach(profile)
            // Disconnected or replaced meanwhile: the newer task owns the state now.
            guard !Task.isCancelled else { return }
            switch outcome {
            case .attached:
                backoff[id] = nil
                tasks[id] = nil
                return
            case let .stop(final):
                status[id] = final
                tasks[id] = nil
                return
            case let .retry(message):
                let delay = backoff[id] ?? 1
                backoff[id] = min(delay * 2, 30)
                status[id] = .retrying(message)
                // Full jitter, like the device relay backoff.
                try? await Task.sleep(for: .seconds(Double.random(in: 0.5...max(0.5, delay))))
            }
        }
    }

    private func attach(_ profile: SSHProfile) async -> Outcome {
        let id = profile.id
        if let problem = SSH.validate(profile) { return .stop(.failed(problem)) }
        status[id] = .connecting("Reading SSH settings…")
        let config = await ProcessRunner.run(SSH.ssh, SSH.resolveArguments(profile))
        guard !Task.isCancelled else { return .stop(.idle) }
        guard config.status == 0, let resolved = SSH.parseConfig(config.text) else {
            return .stop(.failed("SSH couldn't read the settings for \(profile.host). \(config.detail)"))
        }

        // Host key (never by reading ssh's error text).
        let recorded = await recordedKeys(resolved.knownHostsName)
        if recorded.isEmpty {
            if resolved.usesProxy {
                return .stop(.failed("\(profile.host) goes through a jump host. Connect once with ssh in Terminal to confirm its host key, then try again."))
            }
            status[id] = .connecting("Reading the host key…")
            let scan = await ProcessRunner.run(SSH.keyscan, SSH.keyscanArguments(resolved))
            let lines = SSH.knownHostsLines(scan: scan.text, name: resolved.knownHostsName)
            guard !lines.isEmpty else { return .retry("Can't reach \(resolved.hostname):\(resolved.port).") }
            let prints = await ProcessRunner.run(SSH.keygen, ["-l", "-f", "-"], input: Data((lines.joined(separator: "\n") + "\n").utf8))
            let fingerprints = prints.text.split(whereSeparator: \.isNewline).map(String.init)
            guard prints.status == 0, !fingerprints.isEmpty else { return .stop(.failed("Couldn't read the host key of \(profile.host).")) }
            return .stop(.confirmHostKey(name: resolved.knownHostsName, fingerprints: fingerprints, lines: lines))
        }

        status[id] = .connecting("Signing in…")
        let result = await ProcessRunner.run(SSH.ssh, SSH.infoArguments(profile, home: SSH.home))
        guard !Task.isCancelled else { return .stop(.idle) }
        if result.status == 255 {
            if !resolved.usesProxy {
                let scan = await ProcessRunner.run(SSH.keyscan, SSH.keyscanArguments(resolved))
                let current = SSH.hostKeys(scan.text)
                if !current.isEmpty, current.isDisjoint(with: recorded) {
                    return .stop(.failed("The host key of \(profile.host) changed. This can mean someone is intercepting the connection. Codync won't connect until the old key is removed from known_hosts."))
                }
            }
            return .retry("SSH couldn't connect to \(profile.host). \(result.detail)")
        }
        if result.status == 127 { return .stop(.notInstalled) }
        guard let info = try? JSONDecoder().decode(RemoteInfo.self, from: result.stdout) else {
            if result.status == 0 { return .stop(.notInstalled) }
            return .stop(.failed("codync-host on \(profile.host) couldn't report its identity. Run `codync-host install` there. \(result.detail)"))
        }
        let computer = Computer(id: info.computerId, name: info.name, signKey: info.signKey, boxKey: info.boxKey)
        guard computer.isConsistent else { return .stop(.failed("\(profile.host) reported an invalid identity.")) }
        if let known = profile.computerId, known != info.computerId {
            return .stop(.failed("\(profile.host) is now a different computer than the one saved. If it was reinstalled, remove it and add it again."))
        }
        if profile.computerId == nil { remember(info.computerId, name: info.name, for: id) }
        guard info.running else {
            return .retry("codync-host is installed on \(info.name) but not running. Run `codync-host install` there.")
        }

        // Tunnel: a free loopback port each attempt; a port taken in between just makes ssh exit (ExitOnForwardFailure).
        var lastProblem = ""
        for _ in 0..<3 {
            guard let port = Self.freeLocalPort() else { return .retry("No free local port for the tunnel.") }
            status[id] = .connecting("Opening the tunnel…")
            let process: Process
            let errPipe = Pipe()
            do {
                process = try startTunnel(profile, localPort: port, stderr: errPipe)
            } catch {
                return .stop(.failed("Couldn't start ssh: \(error.localizedDescription)"))
            }
            let baseURL = URL(string: "http://127.0.0.1:\(port)")!
            var health: HostHealth?
            for _ in 0..<60 where process.isRunning && !Task.isCancelled {
                if let h = await HostHealth.fetch(baseURL, timeout: 1) {
                    health = h
                    break
                }
                try? await Task.sleep(for: .milliseconds(250))
            }
            guard !Task.isCancelled else {
                process.terminate()
                return .stop(.idle)
            }
            guard let health else {
                // Only read ssh's message once it exited: reading a live pipe would block.
                if process.isRunning {
                    process.terminate()
                } else {
                    let err = String(decoding: errPipe.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
                    lastProblem = err.split(whereSeparator: \.isNewline).last.map(String.init) ?? ""
                }
                continue
            }
            guard health.computerId == info.computerId else {
                process.terminate()
                return .stop(.failed("The tunnel to \(profile.host) reached a different computer than expected."))
            }
            // /health is public, so the answer alone doesn't prove it came through ssh: another
            // local user could have taken the port. Only a listener ssh owns gets the token.
            guard await Self.sshListens(pid: process.processIdentifier, port: port), process.isRunning else {
                log.error("tunnel port \(port) answered by something other than ssh")
                process.terminate()
                lastProblem = "Another program answered on the tunnel's local port."
                continue
            }
            watch(process, id: id, stderr: errPipe)
            tunnels[id] = process
            connectedAt[id] = .now
            status[id] = .connected(localPort: port)
            let attachment = Attachment(computer: computer, baseURL: baseURL, token: info.token)
            attachments[id] = attachment
            onAttach?(attachment)
            return .attached
        }
        return .retry("Couldn't open the tunnel to \(profile.host). \(lastProblem)")
    }

    private func recordedKeys(_ name: String) async -> Set<String> {
        var keys = Set<String>()
        for file in SSH.knownHostsFiles(home: SSH.home) where FileManager.default.fileExists(atPath: file) {
            let found = await ProcessRunner.run(SSH.keygen, ["-F", name, "-f", file])
            if found.status == 0 { keys.formUnion(SSH.hostKeys(found.text)) }
        }
        return keys
    }

    private func startTunnel(_ profile: SSHProfile, localPort: Int, stderr: Pipe) throws -> Process {
        let process = Process()
        process.executableURL = SSH.ssh
        process.arguments = SSH.tunnelArguments(profile, localPort: localPort, home: SSH.home)
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = stderr
        try process.run()
        return process
    }

    /// A tunnel that drops is reopened with backoff (reset once it stayed up a minute).
    private func watch(_ process: Process, id: UUID, stderr: Pipe) {
        let handle = stderr.fileHandleForReading
        process.terminationHandler = { [weak self] _ in
            let detail = String(decoding: handle.readDataToEndOfFile(), as: UTF8.self)
                .split(whereSeparator: \.isNewline).last.map(String.init) ?? ""
            Task { @MainActor [weak self] in self?.tunnelEnded(id, detail: detail) }
        }
    }

    private func tunnelEnded(_ id: UUID, detail: String) {
        guard tunnels[id] != nil else { return }
        log.info("ssh tunnel ended: \(detail, privacy: .public)")
        if let since = connectedAt[id], Date.now.timeIntervalSince(since) >= 60 { backoff[id] = nil }
        stopTunnel(id)
        status[id] = .retrying("The SSH connection dropped. \(detail)")
        connect(id)
    }

    private func stopTunnel(_ id: UUID) {
        if let process = tunnels.removeValue(forKey: id) {
            process.terminationHandler = nil
            process.terminate()
        }
        connectedAt[id] = nil
        if let attachment = attachments.removeValue(forKey: id) { onDetach?(attachment.computer.id) }
    }

    private func remember(_ computerId: ComputerID, name: String, for id: UUID) {
        guard let i = profiles.firstIndex(where: { $0.id == id }) else { return }
        profiles[i].computerId = computerId
        if profiles[i].name.isEmpty { profiles[i].name = name }
        persist()
    }

    private func persist() {
        if let data = try? JSONEncoder().encode(profiles) { UserDefaults.standard.set(data, forKey: Self.defaultsKey) }
    }

    /// Whether process `pid` holds the listening socket on 127.0.0.1:`port`.
    nonisolated static func sshListens(pid: Int32, port: Int) async -> Bool {
        let found = await ProcessRunner.run(SSH.lsof, ["-a", "-n", "-P", "-p", String(pid), "-iTCP@127.0.0.1:\(port)", "-sTCP:LISTEN", "-t"])
        return found.status == 0 && found.text.split(whereSeparator: \.isNewline).contains(Substring(String(pid)))
    }

    /// Binds port 0 on loopback to learn a free port.
    nonisolated static func freeLocalPort() -> Int? {
        let fd = socket(AF_INET, SOCK_STREAM, 0)
        guard fd >= 0 else { return nil }
        defer { close(fd) }
        var addr = sockaddr_in()
        addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = 0
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")
        var len = socklen_t(MemoryLayout<sockaddr_in>.size)
        let bound = withUnsafeMutablePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
                bind(fd, sa, len) == 0 && getsockname(fd, sa, &len) == 0
            }
        }
        guard bound else { return nil }
        return Int(UInt16(bigEndian: addr.sin_port))
    }
}
