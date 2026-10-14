import Foundation
import MetricKit

/// Crashes as `$exception` events for PostHog Error Tracking. MetricKit hands the app its own
/// crash reports on the next launch; each becomes one event with the exception and signal names
/// and binary + offset frames, never a message, reason or memory detail.
public final class CrashReports: NSObject, MXMetricManagerSubscriber, Sendable {
    public static let shared = CrashReports()
    private static let maxFrames = 50

    /// Subscribes once, at app launch (widgets and the notification extension never do).
    public func start() {
        MXMetricManager.shared.add(self)
    }

    public func didReceive(_ payloads: [MXDiagnosticPayload]) {
        let events = payloads.flatMap { $0.crashDiagnostics ?? [] }.map {
            Self.properties(callStack: $0.callStackTree.jsonRepresentation(), exceptionType: $0.exceptionType?.intValue,
                            signal: $0.signal?.intValue, appVersion: $0.applicationVersion)
        }
        guard !events.isEmpty else { return }
        Task { @MainActor in
            events.forEach { Analytics.shared.record("$exception", $0) }
            Analytics.shared.flush()
        }
    }

    /// The `$exception` properties for one crash.
    static func properties(callStack: Data, exceptionType: Int?, signal: Int?, appVersion: String) -> [String: Analytics.Value] {
        let exception: [String: Analytics.Value] = [
            "type": .string(exceptionType.flatMap { machExceptions[$0] } ?? "Crash"),
            "value": .string(signal.flatMap { signals[$0] } ?? ""),
            "mechanism": .object(["type": .string("metrickit"), "handled": .bool(false), "synthetic": .bool(false)]),
            "stacktrace": .object(["type": .string("raw"), "frames": .array(frames(callStack))]),
        ]
        return [
            "$exception_list": .array([.object(exception)]),
            "$exception_level": .string("fatal"),
            "process": .string("ios"),
            "app_version": .string(appVersion),
        ]
    }

    /// The crashed thread's frames, oldest first and the crashing frame last (PostHog's order).
    static func frames(_ callStack: Data) -> [Analytics.Value] {
        guard let tree = try? JSONDecoder().decode(Tree.self, from: callStack),
              let stack = tree.callStacks.first(where: { $0.threadAttributed == true }) ?? tree.callStacks.first
        else { return [] }
        // Root frame is where it crashed; each first sub-frame is its caller.
        var walk: [Frame] = []
        var next = stack.callStackRootFrames.first
        while let frame = next, walk.count < maxFrames {
            walk.append(frame)
            next = frame.subFrames?.first
        }
        return walk.reversed().map { frame in
            let binary = frame.binaryName ?? "???"
            return .object([
                "platform": .string("custom"),
                "lang": .string("swift"),
                "function": .string("\(binary) + \(frame.offsetIntoBinaryTextSegment ?? 0)"),
                "module": .string(binary),
                "resolved": .bool(false),
                "in_app": .bool(binary.hasPrefix("Codync")),
            ])
        }
    }

    private struct Tree: Decodable {
        let callStacks: [Stack]
    }

    private struct Stack: Decodable {
        let threadAttributed: Bool?
        let callStackRootFrames: [Frame]
    }

    private struct Frame: Decodable {
        let binaryName: String?
        let offsetIntoBinaryTextSegment: Int?
        let subFrames: [Frame]?
    }

    /// `<mach/exception_types.h>`.
    static let machExceptions: [Int: String] = [
        1: "EXC_BAD_ACCESS", 2: "EXC_BAD_INSTRUCTION", 3: "EXC_ARITHMETIC", 4: "EXC_EMULATION",
        5: "EXC_SOFTWARE", 6: "EXC_BREAKPOINT", 7: "EXC_SYSCALL", 8: "EXC_MACH_SYSCALL",
        9: "EXC_RPC_ALERT", 10: "EXC_CRASH", 11: "EXC_RESOURCE", 12: "EXC_GUARD", 13: "EXC_CORPSE_NOTIFY",
    ]

    static let signals: [Int: String] = [
        1: "SIGHUP", 2: "SIGINT", 3: "SIGQUIT", 4: "SIGILL", 5: "SIGTRAP", 6: "SIGABRT", 7: "SIGEMT",
        8: "SIGFPE", 9: "SIGKILL", 10: "SIGBUS", 11: "SIGSEGV", 12: "SIGSYS", 13: "SIGPIPE", 14: "SIGALRM", 15: "SIGTERM",
    ]
}
