import SwiftUI

/// One place for "how full is this limit" colors, shared by the apps, the menu bar and widgets.
public extension Palette {
    /// Fill for bars: ink, amber from 70%, red from 90%.
    static func usageFill(_ percent: Double) -> Color {
        percent >= 90 ? danger : percent >= 70 ? warning : accentFill
    }

    /// Tint for gauges and progress views .
    static func usageTint(_ percent: Double) -> Color {
        percent >= 90 ? danger : percent >= 70 ? warning : accent
    }
}

public struct UsageBar: View {
    let percent: Double
    let height: CGFloat

    public init(percent: Double, height: CGFloat = 6) {
        self.percent = percent
        self.height = height
    }

    public var body: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule().fill(Palette.bubbleAgent)
                Capsule()
                    .fill(Palette.usageFill(percent))
                    .frame(width: geo.size.width * min(1, max(0.02, percent / 100)))
            }
        }
        .frame(height: height)
    }
}

public extension UsageProvider {
    /// Each provider's own color, used by the Usage tab and the provider widget.
    var tint: Color {
        switch id {
        case "claude": Color(hex: 0xD97757)
        case "codex": Color(hex: 0x3D8BF2)
        default: Palette.accent
        }
    }

    /// ACP registry id of the provider's agent, for its logo (`AgentIcon`).
    /// Claude and Codex run through ACP adapters (`claude-acp`, `codex-acp`); others by their own id.
    var registry: String { ["claude", "codex"].contains(id) ? "\(id)-acp" : id }

    /// The character that stands for this provider.
    var mascotShape: String { id == "codex" ? "hex" : "blob" }

    var tightest: UsageWindow? { windows.max { $0.percent < $1.percent } }
}

public extension UsageWindow {
    /// "Session" for the 5-hour window, "Opus" for "Weekly · Opus", else the label.
    var title: String {
        if label == "5-hour" { return "Session" }
        if let model = label.split(separator: " · ").dropFirst().first { return String(model) }
        return label
    }

    /// Window length, when the label says it: "5h", "7d".
    var span: String? {
        label == "5-hour" ? "5h" : label.hasPrefix("Weekly") ? "7d" : nil
    }

    /// "resets 3h 20m" / "resets Sep 26 at 12pm", compact for widgets and cards.
    func resetsShort(now: Date = .now) -> String? {
        if let d = resetDate { return d > now ? "resets \(RelativeTime.until(d, now: now))" : nil }
        return resetDescription
    }
}

/// A provider's character, with a red "!" once any of its limits is nearly used up.
public struct ProviderMascot: View {
    let provider: UsageProvider
    let size: CGFloat

    public init(_ provider: UsageProvider, size: CGFloat) {
        self.provider = provider
        self.size = size
    }

    public var body: some View {
        let full = (provider.tightest?.percent ?? 0) >= 90
        CharacterAvatar(shape: provider.mascotShape, tint: provider.tint, size: size, mood: full ? .needsInput : .idle)
            .overlay(alignment: .topTrailing) {
                if full {
                    Image(systemName: "exclamationmark.circle.fill")
                        .font(.system(size: size * 0.34, weight: .bold))
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white, Palette.danger)
                        .offset(x: size * 0.12, y: -size * 0.12)
                }
            }
    }
}
