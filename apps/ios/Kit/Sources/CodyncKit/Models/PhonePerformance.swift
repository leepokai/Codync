import Foundation

/// Measurements sent to this phone's connected host for its local performance log. No row, bot or message identity.
public enum PhonePerformance: Encodable, Sendable {
    case chatOpen(durationMs: Int)
    case scroll(Scroll)

    public struct Scroll: Equatable, Sendable {
        public enum List: String, Sendable { case bots, chat }
        public enum Stop: String, Sendable { case top, end, middle }
        public let list: List
        public let rows: Int
        public let frames: Int
        public let hitches: Int
        public let longestMs: Int
        public let hitchMsPerS: Int
        public let fps: Int
        public let stop: Stop?
        public let jump: Int?

        public init(list: List, rows: Int, frames: Int, hitches: Int, longestMs: Int, hitchMsPerS: Int, fps: Int,
                    stop: Stop? = nil, jump: Int? = nil) {
            self.list = list
            self.rows = rows
            self.frames = frames
            self.hitches = hitches
            self.longestMs = longestMs
            self.hitchMsPerS = hitchMsPerS
            self.fps = fps
            self.stop = stop
            self.jump = jump
        }
    }

    private enum CodingKeys: String, CodingKey {
        case type, platform, durationMs, list, rows, frames, hitches, longestMs, hitchMsPerS, fps, stop, jump
    }

    public func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode("ios", forKey: .platform)
        switch self {
        case let .chatOpen(durationMs):
            try container.encode("chat_open", forKey: .type)
            try container.encode(durationMs, forKey: .durationMs)
        case let .scroll(scroll):
            try container.encode("scroll", forKey: .type)
            try container.encode(scroll.list.rawValue, forKey: .list)
            try container.encode(scroll.rows, forKey: .rows)
            try container.encode(scroll.frames, forKey: .frames)
            try container.encode(scroll.hitches, forKey: .hitches)
            try container.encode(scroll.longestMs, forKey: .longestMs)
            try container.encode(scroll.hitchMsPerS, forKey: .hitchMsPerS)
            try container.encode(scroll.fps, forKey: .fps)
            try container.encodeIfPresent(scroll.stop?.rawValue, forKey: .stop)
            try container.encodeIfPresent(scroll.jump, forKey: .jump)
        }
    }
}

/// One chat's wait from its opening action to its messages' first laid-out frame. Reports only its own first frame.
public struct OpenToMessages {
    private var opening: (botId: String, startedAt: ContinuousClock.Instant)?

    public init() {}

    public mutating func open(_ botId: String, at now: ContinuousClock.Instant = .now) {
        guard opening?.botId != botId else { return }
        opening = (botId, now)
    }

    public mutating func cancel() {
        opening = nil
    }

    public mutating func finish(_ botId: String, at now: ContinuousClock.Instant = .now) -> Int? {
        guard let opening, opening.botId == botId else { return nil }
        self.opening = nil
        let elapsed = (now - opening.startedAt).components
        return Int(elapsed.seconds * 1000 + elapsed.attoseconds / 1_000_000_000_000_000)
    }
}
