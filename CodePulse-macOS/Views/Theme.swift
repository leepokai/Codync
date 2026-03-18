import SwiftUI

struct CodePulseTheme {
    let isDark: Bool
    let isPanel: Bool

    init(isDark: Bool, isPanel: Bool = false) {
        self.isDark = isDark
        self.isPanel = isPanel
    }

    var background: Color {
        if isPanel { return .black }
        return isDark ? Color(red: 0.11, green: 0.11, blue: 0.12) : .white
    }
    var cardBackground: Color {
        if isPanel { return Color.white.opacity(0.08) }
        return isDark ? Color.white.opacity(0.05) : Color.black.opacity(0.03)
    }
    var primaryText: Color {
        if isPanel { return Color(red: 0.96, green: 0.96, blue: 0.97) }
        return isDark ? Color(red: 0.96, green: 0.96, blue: 0.97) : Color(red: 0.11, green: 0.11, blue: 0.12)
    }
    var secondaryText: Color {
        if isPanel { return Color(red: 0.56, green: 0.56, blue: 0.58) }
        return isDark ? Color(red: 0.56, green: 0.56, blue: 0.58) : Color(red: 0.53, green: 0.53, blue: 0.55)
    }
    var separator: Color {
        if isPanel { return Color.white.opacity(0.12) }
        return isDark ? Color.white.opacity(0.1) : Color.black.opacity(0.08)
    }
    var accent: Color { .blue }
    var warning: Color { .orange }
}

private struct ThemeKey: EnvironmentKey {
    static let defaultValue = CodePulseTheme(isDark: false)
}

extension EnvironmentValues {
    var theme: CodePulseTheme {
        get { self[ThemeKey.self] }
        set { self[ThemeKey.self] = newValue }
    }
}
