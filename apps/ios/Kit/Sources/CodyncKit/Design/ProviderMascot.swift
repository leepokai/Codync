import SwiftUI

public enum UsageIconStyle: String, CaseIterable, Sendable {
    case character, original

    public var title: String { self == .character ? "Character" : "Original icon" }
}

public extension SharedStore {
    static let usageIconStyleKey = "usageIconStyle"

    static var usageIconStyle: UsageIconStyle {
        get { UserDefaults(suiteName: appGroup)?.string(forKey: usageIconStyleKey).flatMap(UsageIconStyle.init(rawValue:)) ?? .character }
        set { UserDefaults(suiteName: appGroup)?.set(newValue.rawValue, forKey: usageIconStyleKey) }
    }
}

/// "How full is this limit" colors, shared by the app and its widgets.
public extension Palette {
    /// Tint for gauges and progress views.
    static func usageTint(_ percent: Double) -> Color {
        percent >= 90 ? danger : percent >= 70 ? warning : accent
    }
}

public extension UsageProvider {
    var statusLabel: String? {
        switch status {
        case "unsupported": "Update this agent to read its plan limits."
        case nil: nil
        default: "Plan limits are unavailable for this account."
        }
    }
    /// Claude keeps its orange; everyone else is ink, like the rest of the app.
    var tint: Color { id == "claude" ? Color(hex: 0xD97757) : Palette.accent }

    /// ACP registry id of the provider's agent, for its logo (`AgentIcon`).
    /// Claude and Codex run through ACP adapters (`claude-acp`, `codex-acp`); others by their own id.
    var registry: String {
        let backend = String(id.split(separator: ":", maxSplits: 1).first ?? "")
        return ["claude", "codex", "antigravity"].contains(backend) ? "\(backend)-acp" : backend
    }

    /// The character that stands for this provider.
    var mascotShape: String { id == "codex" ? "hex" : "blob" }

    /// Plan windows. A note (today's tokens, a dollar balance) is not a limit.
    var limits: [UsageWindow] { windows.filter { $0.text == nil } }

    var tightest: UsageWindow? { limits.max { $0.percent < $1.percent } }
}

public extension UsageWindow {
    /// "Session" for the 5-hour window, "Opus" for "Weekly · Opus", else the label.
    var title: String {
        if label == "5-hour" { return "Session" }
        if label.hasPrefix("Weekly · ") { return String(label.dropFirst("Weekly · ".count)) }
        return label
    }

    /// "62% used": the share of the cap already spent, said so (a bare percent reads as what's left).
    var usedText: String {
        L10n.tr("{percent}% used").replacingOccurrences(of: "{percent}", with: "\(Int(percent.rounded()))")
    }

    /// The used percent, or the note when this window isn't a share of a cap.
    var reading: String { text ?? usedText }

    /// "resets 3h 20m" / "resets Sep 26 at 12pm", compact for widgets and cards.
    func resetsShort(now: Date = .now) -> String? {
        if let d = resetDate { return d > now ? "resets \(RelativeTime.until(d, now: now))" : nil }
        return resetDescription
    }
}

public extension Usage {
    /// The spent window that stops a model of `backend` (an account's provider id before its
    /// colon, or the provider's source), until its reset comes.
    func spentWindow(for backend: String, now: Date = .now) -> UsageWindow? {
        providers
            .filter { $0.id.split(separator: ":", maxSplits: 1).first.map(String.init) == backend || $0.source == backend }
            .flatMap(\.limits)
            .filter { $0.percent >= 100 && ($0.resetDate.map { $0 > now } ?? true) }
            .max { $0.percent < $1.percent }
    }

    /// "Usage limit reached · resets 3h 20m", when a spent window stops a model of `backend`.
    func spentNote(for backend: String, now: Date = .now) -> String? {
        guard let window = spentWindow(for: backend, now: now) else { return nil }
        guard let reset = window.resetsShort(now: now) else { return L10n.tr("Usage limit reached") }
        return L10n.tr("Usage limit reached · {reset}").replacingOccurrences(of: "{reset}", with: reset)
    }
}

/// A provider's character, with a red "!" once any of its limits is nearly used up.
public struct ProviderMascot: View {
    let provider: UsageProvider
    let size: CGFloat
    var style: UsageIconStyle = .character

    public init(_ provider: UsageProvider, size: CGFloat, style: UsageIconStyle = .character) {
        self.provider = provider
        self.size = size
        self.style = style
    }

    public var body: some View {
        let full = (provider.tightest?.percent ?? 0) >= 90
        Group {
            if style == .original {
                Image("agent-\(provider.registry)", bundle: .module)
                    .resizable().scaledToFit().padding(size * 0.08)
                    .foregroundStyle(provider.tint)
            } else {
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
        .frame(width: size, height: size)
    }
}
