import Foundation

/// A subagent, as its launching tool call reports it (Claude Code only): the totals once it finished in the foreground.
public struct Subagent: Codable, Hashable, Sendable {
    public var kind: String?
    public var model: String?
    public var background: Bool?
    public var tokens: Int?
    public var toolUses: Int?
    public var durationMs: Int?

    public init(kind: String? = nil, background: Bool? = nil, tokens: Int? = nil, durationMs: Int? = nil) {
        self.kind = kind
        self.background = background
        self.tokens = tokens
        self.durationMs = durationMs
    }

    /// Lenient: a field of a shape this app doesn't know is left out, never the entry.
    public init(from decoder: any Decoder) throws {
        let c = try? decoder.container(keyedBy: CodingKeys.self)
        kind = try? c?.decodeIfPresent(String.self, forKey: .kind)
        model = try? c?.decodeIfPresent(String.self, forKey: .model)
        background = try? c?.decodeIfPresent(Bool.self, forKey: .background)
        tokens = try? c?.decodeIfPresent(Int.self, forKey: .tokens)
        toolUses = try? c?.decodeIfPresent(Int.self, forKey: .toolUses)
        durationMs = try? c?.decodeIfPresent(Int.self, forKey: .durationMs)
    }

    /// Its launch, quietly: its kind, then the tokens it used and how long it ran once it finished, or that it runs in
    /// the background. `nil` while a foreground one runs.
    public var line: String? {
        guard background == true || tokens != nil else { return nil }
        var parts = kind.map { [$0] } ?? []
        if background == true { parts.append(L10n.tr("in background")) }
        if let tokens { parts.append(L10n.fill(L10n.tr("{tokens} tokens"), ["tokens": Self.compact(tokens)])) }
        if let durationMs {
            let s = max(1, Int((Double(durationMs) / 1000).rounded()))
            parts.append(s < 60
                ? L10n.fill(L10n.tr("{count} s"), ["count": String(s)])
                : L10n.fill(L10n.tr("{count} min"), ["count": String(Int((Double(s) / 60).rounded()))]))
        }
        return parts.joined(separator: " · ")
    }

    /// 812, 42.3k, 1.2M.
    static func compact(_ n: Int) -> String {
        guard n >= 1000 else { return String(n) }
        let (div, unit) = n < 1_000_000 ? (1000.0, "k") : (1_000_000.0, "M")
        let tenths = Int((Double(n) / (div / 10)).rounded())
        guard tenths < 1000 else { return String(Int((Double(n) / div).rounded())) + unit }
        return (tenths % 10 == 0 ? String(tenths / 10) : "\(tenths / 10).\(tenths % 10)") + unit
    }
}
