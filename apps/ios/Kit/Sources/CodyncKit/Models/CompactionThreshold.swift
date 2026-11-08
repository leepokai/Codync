import Foundation

/// When Codync summarizes a bot's chat ahead of time (the bot's `compaction`): `"early"`, `"nearLimit"`, `"off"`, or a
/// size in tokens. `"off"` (the apps' Auto) is the default: the agent summarizes on its own. A value from a newer host
/// reads as the default.
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
        case "early": self = .early
        case "nearLimit": self = .nearLimit
        default: self = .off
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

/// The sizes a bot's `CompactionThreshold` may take (`compactionTokens`): `min` to `max` tokens by `step`, and the few
/// its menu lists.
public struct TokenRange: Codable, Hashable, Sendable {
    public var min: Int
    public var max: Int
    public var step: Int
    public var presets: [Int]

    public init(min: Int, max: Int, step: Int, presets: [Int] = []) {
        self.min = min
        self.max = max
        self.step = step
        self.presets = presets
    }

    /// A size typed as `tokens`, as one this window takes: to the nearest step, within `min` and `max`.
    public func fit(_ tokens: Int) -> Int {
        let step = Swift.max(step, 1)
        return Swift.min(Swift.max((tokens + step / 2) / step * step, min), max)
    }

    /// What the user typed, in thousands of tokens ("350", "350K", "1.5k"), as tokens; nil for anything else.
    public static func typed(_ text: String) -> Int? {
        var number = text.trimmingCharacters(in: .whitespaces)
        if number.last == "k" || number.last == "K" { number.removeLast() }
        // Past a billion thousands, no window holds it, and Int(_:) would trap.
        guard let thousands = Double(number), thousands > 0, thousands < 1e9 else { return nil }
        return Int((thousands * 1000).rounded())
    }
}
