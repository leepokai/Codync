import SwiftUI
import CodePulseShared

/// CodePulse iOS design system — Vercel/Cursor inspired, dark-only.
/// Pure black background + #ededed foreground with opacity hierarchy.
struct CodePulseTheme: Sendable {
    // Single-source foreground — #ededed varied by opacity
    private static let fg = Color(red: 0.93, green: 0.93, blue: 0.93)

    // MARK: - Backgrounds

    var background: Color { Color(red: 0.06, green: 0.06, blue: 0.07) }
    var cardBackground: Color { Color.white.opacity(0.05) }
    var hoverBackground: Color { Color.white.opacity(0.06) }
    var rowBackground: Color { Color.white.opacity(0.03) }

    // MARK: - Text hierarchy

    var primaryText: Color { Self.fg }
    var secondaryText: Color { Self.fg.opacity(0.55) }
    var tertiaryText: Color { Self.fg.opacity(0.35) }

    // MARK: - Borders

    var separator: Color { Color.white.opacity(0.08) }
    var border: Color { Color.white.opacity(0.08) }

    // MARK: - Semantic

    var accent: Color { Color(red: 0.35, green: 0.56, blue: 1.0) }
    var warning: Color { Self.fg }
    var danger: Color { Self.fg }

    func waitingColor(for reason: WaitingReason?) -> Color {
        reason?.isPermission == true ? danger : warning
    }
}

private struct ThemeKey: EnvironmentKey {
    static let defaultValue = CodePulseTheme()
}

extension EnvironmentValues {
    var theme: CodePulseTheme {
        get { self[ThemeKey.self] }
        set { self[ThemeKey.self] = newValue }
    }
}
