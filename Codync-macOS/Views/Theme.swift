import SwiftUI
import CodyncShared

/// Codync design system inspired by Vercel/Cursor.
/// Panel mode: pure black + single-source opacity hierarchy.
/// Standard mode: light/dark adaptive colors.
struct CodyncTheme: GlassThemeProvider, Sendable {
    let isDark: Bool
    let isPanel: Bool

    init(isDark: Bool, isPanel: Bool = false) {
        self.isDark = isDark
        self.isPanel = isPanel
    }

    // MARK: - Base foreground (single-source opacity system for panel)

    /// Panel uses #ededed as the single source color, varied by opacity.
    private static let panelForeground = Color(red: 0.93, green: 0.93, blue: 0.93)

    // MARK: - Backgrounds

    var background: Color {
        if isPanel { return isDark ? .black : Color(red: 0.95, green: 0.95, blue: 0.96) }
        return isDark ? Color(red: 0.11, green: 0.11, blue: 0.12) : .white
    }

    var glassBackground: Color {
        isDark ? Color.white.opacity(0.05) : Color.white.opacity(0.70)
    }

    /// Elevated surface for cards/stats — Vercel uses #111 or white 4-5%
    var cardBackground: Color {
        if isPanel { return .white.opacity(0.10) }
        return isDark ? .white.opacity(0.05) : .black.opacity(0.03)
    }

    /// Hover/press state — Cursor uses white 6%
    var hoverBackground: Color {
        if isPanel { return .white.opacity(0.06) }
        return isDark ? .white.opacity(0.06) : .black.opacity(0.04)
    }

    var pressedBackground: Color {
        if isPanel { return .white.opacity(0.10) }
        return isDark ? .white.opacity(0.10) : .black.opacity(0.08)
    }

    // MARK: - Text hierarchy (opacity-based for panel)

    var primaryText: Color {
        if isPanel { return Self.panelForeground }
        return isDark ? Color(red: 0.93, green: 0.93, blue: 0.93) : Color(red: 0.11, green: 0.11, blue: 0.12)
    }

    /// 55% opacity — Cursor's secondary text
    var secondaryText: Color {
        if isPanel { return Self.panelForeground.opacity(0.55) }
        return isDark ? .white.opacity(0.55) : .black.opacity(0.50)
    }

    /// 35% opacity — captions, timestamps
    var tertiaryText: Color {
        if isPanel { return Self.panelForeground.opacity(0.35) }
        return isDark ? .white.opacity(0.35) : .black.opacity(0.35)
    }

    // MARK: - Borders & separators — Vercel uses #242424 ≈ white 8%

    var separator: Color {
        if isPanel { return .white.opacity(0.08) }
        return isDark ? .white.opacity(0.10) : .black.opacity(0.08)
    }

    var border: Color {
        if isPanel { return .white.opacity(0.08) }
        return isDark ? .white.opacity(0.10) : .black.opacity(0.10)
    }

    var borderHover: Color {
        if isPanel { return .white.opacity(0.15) }
        return isDark ? .white.opacity(0.18) : .black.opacity(0.18)
    }

    // MARK: - Semantic colors

    var accent: Color { primaryText }

    var warning: Color { primaryText }
    var danger: Color { primaryText }

    /// Color for a waiting session based on its reason.
    /// Permission prompt = danger (red), everything else = warning (yellow/orange).
    func waitingColor(for reason: WaitingReason?) -> Color {
        reason?.isPermission == true ? danger : warning
    }
}

// MARK: - Environment

private struct ThemeKey: EnvironmentKey {
    static let defaultValue = CodyncTheme(isDark: false)
}

extension EnvironmentValues {
    var theme: CodyncTheme {
        get { self[ThemeKey.self] }
        set { self[ThemeKey.self] = newValue }
    }
}
