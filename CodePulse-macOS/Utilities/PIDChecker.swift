import Foundation

enum PIDChecker {
    static func isAlive(pid: Int) -> Bool {
        kill(Int32(pid), 0) == 0
    }

    /// Get the TTY device for a PID by walking up the process tree.
    /// Uses a single `ps -o tty=,ppid=` per iteration (max 5 subprocesses).
    static func tty(for pid: Int) -> String? {
        var current = Int32(pid)
        for _ in 0..<5 {
            guard let output = psColumns(pid: current, columns: "tty=,ppid=") else { break }
            let parts = output.split(separator: " ", maxSplits: 1)
            let tty = parts.first.map(String.init)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            if !tty.isEmpty && tty != "??" {
                return "/dev/\(tty)"
            }
            guard parts.count > 1,
                  let ppid = Int32(parts[1].trimmingCharacters(in: .whitespacesAndNewlines)),
                  ppid > 0 else { break }
            current = ppid
        }
        return nil
    }

    /// Run `ps -o <columns> -p <pid>` and return the output string.
    private static func psColumns(pid: Int32, columns: String) -> String? {
        let proc = Process()
        proc.executableURL = URL(fileURLWithPath: "/bin/ps")
        proc.arguments = ["-o", columns, "-p", "\(pid)"]
        let pipe = Pipe()
        proc.standardOutput = pipe
        proc.standardError = FileHandle.nullDevice
        guard (try? proc.run()) != nil else { return nil }
        proc.waitUntilExit()
        guard proc.terminationStatus == 0 else { return nil }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        return String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Check if a process was launched with --dangerously-skip-permissions
    static func skipsPermissions(pid: Int) -> Bool {
        guard let args = psColumns(pid: Int32(pid), columns: "args=") else { return false }
        return args.contains("--dangerously-skip-permissions")
    }
}
