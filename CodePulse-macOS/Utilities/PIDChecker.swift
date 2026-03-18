import Foundation

enum PIDChecker {
    static func isAlive(pid: Int) -> Bool {
        kill(Int32(pid), 0) == 0
    }

    /// Check if a process was launched with --dangerously-skip-permissions
    static func skipsPermissions(pid: Int) -> Bool {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/bin/ps")
        proc.arguments = ["-p", "\(pid)", "-o", "args="]
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = FileHandle.nullDevice
        do {
            try proc.run()
            proc.waitUntilExit()
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let args = String(data: data, encoding: .utf8) ?? ""
            return args.contains("--dangerously-skip-permissions")
        } catch {
            return false
        }
    }
}
