import Foundation
import MetricKit

/// Crashes as `$exception` events for PostHog Error Tracking. MetricKit hands the app its own
/// crash reports on the next launch; each becomes one event with the exception and signal names
/// and code addresses, never a message, reason or memory detail.
public final class CrashReports: NSObject, MXMetricManagerSubscriber, Sendable {
    public static let shared = CrashReports()
    /// A SwiftUI update alone runs dozens of frames deep: fewer would cut off the app's own.
    private static let maxFrames = 200

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
            events.forEach { Analytics.shared.recordCrash($0) }
            Analytics.shared.flush()
        }
    }

    /// The `$exception` properties for one crash.
    static func properties(callStack: Data, exceptionType: Int?, signal: Int?, appVersion: String) -> [String: Analytics.Value] {
        let (frames, images) = stack(callStack)
        let exception: [String: Analytics.Value] = [
            "type": .string(exceptionType.flatMap { machExceptions[$0] } ?? "Crash"),
            "value": .string(signal.flatMap { signals[$0] } ?? ""),
            "mechanism": .object(["type": .string("metrickit"), "handled": .bool(false), "synthetic": .bool(false)]),
            "stacktrace": .object(["type": .string("raw"), "frames": .array(frames)]),
        ]
        var properties: [String: Analytics.Value] = [
            "$exception_list": .array([.object(exception)]),
            "$exception_level": .string("fatal"),
            "process": .string("ios"),
            "app_version": .string(appVersion),
        ]
        if !images.isEmpty { properties["$debug_images"] = .array(images) }
        return properties
    }

    /// The crashed thread's frames as PostHog `apple` frames, oldest first and the crashing frame
    /// last (PostHog's order), and the binaries they ran in. PostHog symbolicates them with the
    /// dSYMs each release uploads (tools/upload-dsyms.py); a frame it can't resolve (the system's)
    /// keeps its `binary + offset` name.
    static func stack(_ callStack: Data) -> (frames: [Analytics.Value], images: [Analytics.Value]) {
        guard let tree = try? JSONDecoder().decode(Tree.self, from: callStack),
              let thread = tree.callStacks.first(where: { $0.threadAttributed == true }) ?? tree.callStacks.first
        else { return ([], []) }
        // Root frame is where it crashed; each first sub-frame is its caller.
        var walk: [Frame] = []
        var next = thread.callStackRootFrames.first
        while let frame = next, walk.count < maxFrames {
            walk.append(frame)
            next = frame.subFrames?.first
        }
        var frames: [Analytics.Value] = []
        var images: [Analytics.Value] = []
        var loaded: Set<UInt64> = []
        for frame in walk.reversed() {
            let binary = frame.binaryName ?? "???"
            let offset = frame.offsetIntoBinaryTextSegment ?? 0
            var apple: [String: Analytics.Value] = [
                "platform": .string("apple"),
                "function": .string("\(binary) + \(offset)"),
                "module": .string(binary),
                "in_app": .bool(binary.hasPrefix("Codync")),
            ]
            // Pointer authentication bits off (arm64e); the binary was loaded at address - offset.
            if let address = frame.address.map({ $0 & 0xF_FFFF_FFFF }), offset <= address {
                let base = address - offset
                apple["instruction_addr"] = .string(hex(address))
                apple["image_addr"] = .string(hex(base))
                // PostHog matches the dSYM by its UUID in this exact form (uppercase, dashed).
                if let uuid = frame.binaryUUID.flatMap(UUID.init(uuidString:)), loaded.insert(base).inserted {
                    images.append(.object(["type": .string("macho"), "debug_id": .string(uuid.uuidString), "image_addr": .string(hex(base))]))
                }
            }
            frames.append(.object(apple))
        }
        return (frames, images)
    }

    private static func hex(_ address: UInt64) -> String {
        "0x" + String(address, radix: 16)
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
        let binaryUUID: String?
        let address: UInt64?
        let offsetIntoBinaryTextSegment: UInt64?
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
