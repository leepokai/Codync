import Foundation

/// When Codync summarizes a bot's chat ahead of time (the bot's `compaction`): `"early"`, `"nearLimit"`, `"off"`, or a
/// size in tokens. A value from a newer host reads as the default.
public enum CompactionThreshold: Codable, Hashable, Sendable {
    case early, nearLimit, off
    case tokens(Int)

    public init(from decoder: Decoder) throws {
        let c = try decoder.singleValueContainer()
        if let tokens = try? c.decode(Int.self) {
            self = .tokens(tokens)
            return
        }
        switch try? c.decode(String.self) {
        case "nearLimit": self = .nearLimit
        case "off": self = .off
        default: self = .early
        }
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .early: try c.encode("early")
        case .nearLimit: try c.encode("nearLimit")
        case .off: try c.encode("off")
        case .tokens(let tokens): try c.encode(tokens)
        }
    }
}

/// The sizes a bot's `CompactionThreshold` may take (`compactionTokens`): `min` to `max` tokens by `step`.
public struct TokenRange: Codable, Hashable, Sendable {
    public var min: Int
    public var max: Int
    public var step: Int

    public init(min: Int, max: Int, step: Int) {
        self.min = min
        self.max = max
        self.step = step
    }

    public var sizes: [Int] {
        step > 0 && min <= max ? Array(stride(from: min, through: max, by: step)) : []
    }
}
