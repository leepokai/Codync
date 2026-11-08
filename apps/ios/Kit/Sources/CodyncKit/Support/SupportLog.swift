import Foundation
import Observation

/// What a support log line is about; the level says how loud it is.
public enum SupportLogLevel: String, Sendable {
    case info
    case warn
    case error

    /// The uppercase word in a saved line (`INFO`, `WARN`, `ERROR`).
    public var tag: String { rawValue.uppercased() }
}

/// Where a support log line came from.
public enum SupportLogSource: String, Sendable {
    case app
    case network
    case connection

    public var tag: String { rawValue.uppercased() }
}

/// One logged event; `at` is when it happened.
public struct SupportEvent: Identifiable, Sendable {
    public let id: Int
    public let at: Date
    public let level: SupportLogLevel
    public let source: SupportLogSource
    public let message: String

    public init(id: Int, at: Date, level: SupportLogLevel, source: SupportLogSource, message: String) {
        self.id = id
        self.at = at
        self.level = level
        self.source = source
        self.message = message
    }
}

/// Recent app events for the Support page: connection steps, each request with its outcome and
/// errors, redacted and capped at 5,000 lines. It stays on this iPhone (the cache, never backed up)
/// until the person saves it from Settings → Support or clears it.
///
/// A request may log from any thread, so `record` hops to the main actor, where the page observes
/// the list; the saved file is written every few seconds rather than on every line.
@MainActor
@Observable
public final class SupportLog {
    public static let shared = SupportLog()

    private static let maxEvents = 5_000
    private static let maxMessage = 600
    private static let saveEvery: Duration = .seconds(5)
    /// Oldest first.
    public private(set) var events: [SupportEvent] = []
    private var nextId = 0
    private var file: URL?
    private var dirty = false
    private var saveTask: Task<Void, Never>?

    public init() {}

    /// Restores the log saved on this phone and starts writing changes back.
    public func start() {
        guard file == nil else { return }
        let target = URL.cachesDirectory.appending(path: "support-log.txt")
        file = target
        let lines = (try? String(contentsOf: target, encoding: .utf8))?
            .split(separator: "\n")
            .suffix(Self.maxEvents) ?? []
        let restored = lines.enumerated().compactMap { index, line in
            Self.event(from: line, id: index)
        }
        if !restored.isEmpty {
            events = restored
            nextId = restored.count
        }
        add(.info, .app, "The app started")
    }

    /// Logs without waiting, from any thread.
    public nonisolated static func record(_ level: SupportLogLevel, _ source: SupportLogSource, _ message: String) {
        Task { @MainActor in shared.add(level, source, message) }
    }

    public nonisolated static func info(_ source: SupportLogSource, _ message: String) {
        record(.info, source, message)
    }

    public nonisolated static func warn(_ source: SupportLogSource, _ message: String) {
        record(.warn, source, message)
    }

    public nonisolated static func error(_ source: SupportLogSource, _ message: String) {
        record(.error, source, message)
    }

    public func add(_ level: SupportLogLevel, _ source: SupportLogSource, _ message: String) {
        let text = Self.redact(message)
        let event = SupportEvent(id: nextId, at: .now, level: level, source: source,
                                 message: text.count > Self.maxMessage ? String(text.prefix(Self.maxMessage)) + "…" : text)
        nextId += 1
        events = Array((events + [event]).suffix(Self.maxEvents))
        dirty = true
        scheduleSave()
    }

    public func clear() {
        events = []
        // The next save writes the empty file.
        dirty = true
        scheduleSave()
    }

    /// The saved file's text, newest last, each line stamped and redacted.
    public func fileText() -> String {
        events.map { event in
            let stamp = event.at.formatted(.iso8601)
            return "\(stamp) \(event.level.tag) \(event.source.tag) \(Self.redact(event.message))"
        }.joined(separator: "\n")
    }

    /// Writes the log to `codync-support-<date>.txt` for the share sheet.
    public func exportFile() throws -> URL {
        let stamp = Date.now.formatted(.iso8601.year().month().day())
        let target = URL.temporaryDirectory.appending(path: "codync-support-\(stamp).txt")
        try fileText().write(to: target, atomically: true, encoding: .utf8)
        return target
    }

    private func scheduleSave() {
        guard saveTask == nil else { return }
        saveTask = Task { [weak self] in
            try? await Task.sleep(for: Self.saveEvery)
            guard let self else { return }
            self.saveTask = nil
            guard self.dirty, let file = self.file else { return }
            self.dirty = false
            try? self.fileText().write(to: file, atomically: true, encoding: .utf8)
        }
    }

    private static func event(from line: Substring, id: Int) -> SupportEvent? {
        let parts = line.split(separator: " ", maxSplits: 3)
        guard parts.count == 4, let at = try? Date(String(parts[0]), strategy: .iso8601) else { return nil }
        let level = SupportLogLevel(rawValue: parts[1].lowercased()) ?? .info
        let source = SupportLogSource(rawValue: parts[2].lowercased()) ?? .app
        return SupportEvent(id: id, at: at, level: level, source: source, message: String(parts[3]))
    }

    /// A bearer line keeps its scheme word, a key keeps its prefix and four characters, a long token
    /// four characters, and an address its domain; each message also stays on one line.
    nonisolated(unsafe) private static let rules: [(regex: NSRegularExpression, template: String)] = [
        (try! NSRegularExpression(pattern: #"(?i)(bearer\s+)\S+"#), "$1•••"),
        (try! NSRegularExpression(pattern: #"\b((?:sk|pk|ghp|gho|ghs|xoxb|xoxp|shpat|glpat)_[A-Za-z0-9]{4})[A-Za-z0-9_]{2,}"#), "$1…"),
        (try! NSRegularExpression(pattern: #"(?<![\w/~-])([A-Za-z0-9_~+/=-]{4})[A-Za-z0-9_~+/=-]{24,}(?![\w/~-])"#), "$1…"),
        (try! NSRegularExpression(pattern: #"([\w.+-]{1,2})[\w.+-]*@([\w-]+(?:\.[\w-]+)+)"#), "$1…@$2"),
        (try! NSRegularExpression(pattern: #"\s*[\r\n]+\s*"#), " | "),
        (try! NSRegularExpression(pattern: #"\p{Cc}"#), " "),
    ]

    /// Masks tokens, keys and addresses as the desktop logs do, and keeps each message on one line.
    public nonisolated static func redact(_ text: String) -> String {
        rules.reduce(text) { current, rule in
            let range = NSRange(current.startIndex..., in: current)
            return rule.regex.stringByReplacingMatches(in: current, range: range, withTemplate: rule.template)
        }
    }
}

public extension SupportEvent {
    /// The time to show on the page, e.g. `14:32:07`.
    var clock: String { at.formatted(date: .omitted, time: .standard) }
}
