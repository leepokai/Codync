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
        let (frames, images, problem) = stack(callStack)
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
        // Why a crash came without frames, so the next one can be read: the JSON's own key names, never a value.
        if let problem { properties["stack_problem"] = .string(problem) }
        return properties
    }

    /// The crashed thread's frames as PostHog `apple` frames, oldest first and the crashing frame
    /// last (PostHog's order), and the binaries they ran in. PostHog symbolicates them with the
    /// dSYMs each release uploads (tools/upload-dsyms.py); a frame it can't resolve (the system's)
    /// keeps its `binary + offset` name.
    static func stack(_ callStack: Data) -> (frames: [Analytics.Value], images: [Analytics.Value], problem: String?) {
        let tree: Tree
        do {
            tree = try JSONDecoder().decode(Tree.self, from: callStack)
        } catch {
            return ([], [], "unreadable: " + decodingPlace(error))
        }
        // The crashed thread, or the first with frames when MetricKit attributed none (or an empty one).
        let stacks = (tree.callStacks ?? []).filter { !($0.callStackRootFrames ?? []).isEmpty }
        guard let thread = stacks.first(where: { $0.threadAttributed == true }) ?? stacks.first else {
            return ([], [], tree.callStacks == nil ? "no callStacks" : "no frames")
        }
        // Root frame is where it crashed; each first sub-frame is its caller.
        var walk: [Frame] = []
        var next = thread.callStackRootFrames?.first
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
        return (frames, images, nil)
    }

    /// Where decoding stopped and why, as key names and a kind (`callStacks.0.callStackRootFrames: missing`): no value of the payload.
    private static func decodingPlace(_ error: Error) -> String {
        guard let error = error as? DecodingError else { return "not JSON" }
        let (kind, context): (String, DecodingError.Context) = switch error {
        case .keyNotFound(let key, let c): ("missing \(key.stringValue)", c)
        case .typeMismatch(_, let c): ("type mismatch", c)
        case .valueNotFound(_, let c): ("null", c)
        case .dataCorrupted(let c): ("not JSON", c)
        @unknown default: ("unknown", DecodingError.Context(codingPath: [], debugDescription: ""))
        }
        let path = context.codingPath.map { $0.intValue.map(String.init) ?? $0.stringValue }.joined(separator: ".")
        return path.isEmpty ? kind : "\(path): \(kind)"
    }

    private static func hex(_ address: UInt64) -> String {
        "0x" + String(address, radix: 16)
    }

    private struct Tree: Decodable {
        let callStacks: [Stack]?
    }

    private struct Stack: Decodable {
        let threadAttributed: Bool?
        let callStackRootFrames: [Frame]?
    }

    private struct Frame: Decodable {
        let binaryName: String?
        let binaryUUID: String?
        let address: UInt64?
        let offsetIntoBinaryTextSegment: UInt64?
        let subFrames: [Frame]?

        private enum CodingKeys: String, CodingKey {
            case binaryName, binaryUUID, address, offsetIntoBinaryTextSegment, subFrames
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            binaryName = try? c.decode(String.self, forKey: .binaryName)
            binaryUUID = try? c.decode(String.self, forKey: .binaryUUID)
            address = Self.number(c, .address)
            offsetIntoBinaryTextSegment = Self.number(c, .offsetIntoBinaryTextSegment)
            subFrames = try c.decodeIfPresent([Frame].self, forKey: .subFrames)
        }

        /// An address as a whole number, or as a float when the JSON wrote one: a frame's other fields still count.
        private static func number(_ c: KeyedDecodingContainer<CodingKeys>, _ key: CodingKeys) -> UInt64? {
            if let n = try? c.decode(UInt64.self, forKey: key) { return n }
            if let d = try? c.decode(Double.self, forKey: key), d >= 0, d < 0x1p64 { return UInt64(d) }
            return nil
        }
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
