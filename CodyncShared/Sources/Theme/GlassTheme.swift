import SwiftUI

// MARK: - GlassCardStyle

public enum GlassCardStyle: Sendable {
    case normal
    case elevated
    case receded
}

// MARK: - GlassThemeProvider

public protocol GlassThemeProvider: Sendable {
    var isDark: Bool { get }

    // Backgrounds
    var background: Color { get }
    var cardBackground: Color { get }
    var glassBackground: Color { get }

    // Text
    var primaryText: Color { get }
    var secondaryText: Color { get }
    var tertiaryText: Color { get }

    // Borders
    var separator: Color { get }
    var border: Color { get }

    // Glass tokens — default implementations provided below
    var glassBorder: Color { get }
    var glassInnerHighlight: Color { get }
    var glassShadow: Color { get }
    var glassShadowRadius: CGFloat { get }

    // Semantic
    var accent: Color { get }
    func waitingColor(for reason: WaitingReason?) -> Color
}

// MARK: - Default glass token implementations

public extension GlassThemeProvider {
    var glassBorder: Color {
        isDark
            ? Color.white.opacity(0.18)
            : Color.white.opacity(0.60)
    }

    var glassInnerHighlight: Color {
        isDark
            ? Color.white.opacity(0.12)
            : Color.white.opacity(0.75)
    }

    var glassShadow: Color {
        isDark
            ? Color.black.opacity(0.55)
            : Color.black.opacity(0.18)
    }

    var glassShadowRadius: CGFloat {
        isDark ? 16 : 10
    }
}

// MARK: - GlassCardModifier

public struct GlassCardModifier: ViewModifier {
    public let style: GlassCardStyle
    public let isDark: Bool

    public init(style: GlassCardStyle, isDark: Bool) {
        self.style = style
        self.isDark = isDark
    }

    // Resolved token values

    private var resolvedBackground: Color {
        switch style {
        case .normal:
            return isDark
                ? Color.white.opacity(0.05)
                : Color.white.opacity(0.70)
        case .elevated:
            return isDark
                ? Color.white.opacity(0.10)
                : Color.white.opacity(0.85)
        case .receded:
            return isDark
                ? Color.white.opacity(0.02)
                : Color.white.opacity(0.35)
        }
    }

    private var resolvedBorderColor: Color {
        switch style {
        case .normal:
            return isDark
                ? Color.white.opacity(0.10)
                : Color.black.opacity(0.06)
        case .elevated:
            return isDark
                ? Color.white.opacity(0.25)
                : Color.white.opacity(0.80)
        case .receded:
            return isDark
                ? Color.white.opacity(0.04)
                : Color.black.opacity(0.03)
        }
    }

    private var resolvedBorderWidth: CGFloat {
        switch style {
        case .normal: return 0.5
        case .elevated: return 1.0
        case .receded: return 0.5
        }
    }

    private var resolvedScale: CGFloat {
        switch style {
        case .normal: return 1.0
        case .elevated: return 1.01
        case .receded: return 0.95
        }
    }

    private var resolvedOpacity: Double {
        switch style {
        case .normal, .elevated: return 1.0
        case .receded: return 0.5
        }
    }

    private var shadowRadius: CGFloat {
        isDark ? 16 : 10
    }

    private var shadowColor: Color {
        isDark
            ? Color.black.opacity(0.55)
            : Color.black.opacity(0.18)
    }

    private var innerHighlight: Color {
        isDark
            ? Color.white.opacity(0.12)
            : Color.white.opacity(0.75)
    }

    private let cornerRadius: CGFloat = 14

    public func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)

        content
            .background(resolvedBackground, in: shape)
            .overlay {
                if style == .elevated {
                    LinearGradient(
                        colors: [innerHighlight, Color.clear],
                        startPoint: .top,
                        endPoint: .center
                    )
                    .clipShape(shape)
                }
            }
            .overlay(shape.stroke(resolvedBorderColor, lineWidth: resolvedBorderWidth))
            .shadow(
                color: style == .elevated ? shadowColor : .clear,
                radius: style == .elevated ? shadowRadius : 0,
                x: 0, y: 4
            )
            .scaleEffect(resolvedScale)
            .opacity(resolvedOpacity)
    }
}

// MARK: - View extension

public extension View {
    func glassCard(_ style: GlassCardStyle = .normal, isDark: Bool) -> some View {
        modifier(GlassCardModifier(style: style, isDark: isDark))
    }
}
