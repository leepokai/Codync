import SwiftUI
import CodyncShared

/// Codync iOS design system — Vercel/Cursor inspired, supports dark and light mode.
/// Pure black background + #ededed foreground with opacity hierarchy in dark mode;
/// near-white background + near-black foreground in light mode.
struct CodyncTheme: GlassThemeProvider, Sendable {
    let isDark: Bool

    init(isDark: Bool = true) {
        self.isDark = isDark
    }

    // Single-source foreground colors
    private static let fgDark = Color(red: 0.93, green: 0.93, blue: 0.93)
    private static let fgLight = Color(red: 0.11, green: 0.11, blue: 0.12)

    private var fg: Color { isDark ? Self.fgDark : Self.fgLight }

    // MARK: - Backgrounds

    var background: Color {
        isDark
            ? Color(red: 0.06, green: 0.06, blue: 0.07)
            : Color(red: 0.96, green: 0.96, blue: 0.97)
    }

    var cardBackground: Color {
        isDark ? Color.white.opacity(0.05) : Color.white.opacity(0.70)
    }

    var glassBackground: Color {
        isDark ? Color.white.opacity(0.08) : Color.white.opacity(0.60)
    }

    var hoverBackground: Color {
        isDark ? Color.white.opacity(0.06) : Color.black.opacity(0.04)
    }

    var rowBackground: Color {
        isDark ? Color.white.opacity(0.03) : Color.black.opacity(0.02)
    }

    // MARK: - Text hierarchy

    var primaryText: Color { fg }
    var secondaryText: Color { fg.opacity(0.55) }
    var tertiaryText: Color { fg.opacity(0.35) }

    // MARK: - Borders

    var separator: Color {
        isDark ? Color.white.opacity(0.08) : Color.black.opacity(0.08)
    }

    var border: Color {
        isDark ? Color.white.opacity(0.08) : Color.black.opacity(0.08)
    }

    // MARK: - Semantic

    var accent: Color { fg }
    var warning: Color { fg }
    var danger: Color { fg }

    func waitingColor(for reason: WaitingReason?) -> Color {
        reason?.isPermission == true ? danger : warning
    }
}

private struct ThemeKey: EnvironmentKey {
    static let defaultValue = CodyncTheme(isDark: true)
}

extension EnvironmentValues {
    var theme: CodyncTheme {
        get { self[ThemeKey.self] }
        set { self[ThemeKey.self] = newValue }
    }
}
