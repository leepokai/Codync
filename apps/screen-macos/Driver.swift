import Foundation

/// The computer-use driver bots act through (`cua-driver`, shipped in Contents/Helpers; see
/// `host/src/screen/cua.rs`). We spawn it ourselves, never through `open` or LaunchServices, so
/// macOS treats it as part of Codync Screen: it uses our Accessibility and Screen Recording
/// grants and never asks for its own. It exits with us (its stdin closes).
@MainActor
final class Driver {
    private var process: Process?
    /// Never written: the driver stops when it closes, which happens when we end, however we end.
    private var liveness: Pipe?

    /// Starts the driver if it isn't running and returns its socket.
    func socket() async throws -> String {
        let path = Self.socketPath
        if process?.isRunning == true { return path }
        let exe = Bundle.main.bundleURL.appending(path: "Contents/Helpers/cua-driver")
        guard FileManager.default.isExecutableFile(atPath: exe.path) else {
            throw HelperError("The computer-use driver is missing from Codync Screen. Reinstall Codync.")
        }
        let dir = (path as NSString).deletingLastPathComponent
        try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        try? FileManager.default.removeItem(atPath: path)

        let p = Process()
        p.executableURL = exe
        p.arguments = ["serve", "--socket", path]
        var env = ProcessInfo.processInfo.environment
        env["CUA_DRIVER_EMBEDDED"] = "1"
        env["CUA_DRIVER_PARENT_LIVENESS_STDIN"] = "1"
        env["CUA_DRIVER_HOST_BUNDLE_ID"] = Bundle.main.bundleIdentifier ?? ""
        env["CUA_DRIVER_RS_TELEMETRY_ENABLED"] = "0"
        env["CUA_DRIVER_RS_UPDATE_CHECK"] = "0"
        p.environment = env
        let stdin = Pipe()
        p.standardInput = stdin
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        p.terminationHandler = { [weak self] ended in
            Task { @MainActor in if self?.process === ended { self?.process = nil } }
        }
        try p.run()
        process = p
        liveness = stdin
        for _ in 0..<100 where p.isRunning {
            if Self.listening(path) { return path }
            try await Task.sleep(for: .milliseconds(150))
        }
        p.terminate()
        throw HelperError("The computer-use driver didn't start.")
    }

    /// In the per-user temporary directory (private, and short enough for a socket path).
    private static var socketPath: String {
        let development = Bundle.main.object(forInfoDictionaryKey: "CodyncEnvironment") as? String == "dev"
        return NSTemporaryDirectory() + (development ? "codync-dev-cua/cua.sock" : "codync-cua/cua.sock")
    }

    private static func listening(_ path: String) -> Bool {
        let fd = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { return false }
        defer { close(fd) }
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let capacity = MemoryLayout.size(ofValue: addr.sun_path)
        guard path.utf8.count < capacity else { return false }
        withUnsafeMutableBytes(of: &addr.sun_path) { buf in
            buf.copyBytes(from: path.utf8)
            buf[path.utf8.count] = 0
        }
        return withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) == 0
            }
        }
    }
}
