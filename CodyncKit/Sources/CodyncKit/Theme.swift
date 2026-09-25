import SwiftUI

#if canImport(UIKit)
import UIKit
private typealias PlatformColor = UIColor
#else
import AppKit
private typealias PlatformColor = NSColor
#endif

private extension PlatformColor {
    convenience init(hex: UInt32, alpha: CGFloat = 1) {
        self.init(
            red: CGFloat((hex >> 16) & 0xFF) / 255,
            green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255,
            alpha: alpha
        )
    }
}

public extension Color {
    init(hex: UInt32) {
        self.init(red: Double((hex >> 16) & 0xFF) / 255, green: Double((hex >> 8) & 0xFF) / 255, blue: Double(hex & 0xFF) / 255)
    }

    /// Light/dark pair resolved by the system appearance.
    init(light: UInt32, dark: UInt32) {
        #if canImport(UIKit)
        self.init(uiColor: UIColor { $0.userInterfaceStyle == .dark ? UIColor(hex: dark) : UIColor(hex: light) })
        #else
        self.init(nsColor: NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? NSColor(hex: dark) : NSColor(hex: light)
        })
        #endif
    }
}

/// Black and white: a pure black (or white) ground, grey bubbles, and color that
/// comes only from the bots; amber marks "needs you" and red marks errors.
public enum Palette {
    public static let background = Color(light: 0xFFFFFF, dark: 0x000000)
    public static let surface = Color(light: 0xF4F4F4, dark: 0x141414)
    public static let bubbleAgent = Color(light: 0xF0F0F0, dark: 0x1C1C1C)
    public static let bubbleUser = Color(light: 0xE2E2E2, dark: 0x3A3A3A)
    public static let border = Color(light: 0xE6E6E6, dark: 0x262626)
    public static let text = Color(light: 0x141414, dark: 0xF2F2F2)
    public static let secondary = Color(light: 0x6B6B6B, dark: 0x9A9A9A)
    public static let tertiary = Color(light: 0x9B9B9B, dark: 0x6E6E6E)
    /// Ink used for fills (primary buttons, the send button, unread badges).
    public static let accentFill = Color(light: 0x000000, dark: 0xFFFFFF)
    /// Ink readable as text and tint on the background.
    public static let accent = Color(light: 0x000000, dark: 0xFFFFFF)
    public static let onAccent = Color(light: 0xFFFFFF, dark: 0x000000)
    public static let accentDim = Color(light: 0xD9D9D9, dark: 0x333333)
    public static let danger = Color(light: 0xC23A2B, dark: 0xF0A7A7)
    public static let warning = Color(hex: 0xF0A030)
    public static let codeBackground = Color(light: 0xF4F4F4, dark: 0x111111)
    public static let added = Color(light: 0x2E7D32, dark: 0x8FD18B)
    public static let removed = Color(light: 0xC62828, dark: 0xF0A7A7)
}

public enum AvatarPalette {
    public struct Swatch: Identifiable, Hashable, Sendable {
        public let id: String
        public let label: String
        public let hex: UInt32
        public var color: Color { Color(hex: hex) }
    }

    public static let colors: [Swatch] = [
        .init(id: "black", label: "Black", hex: 0x2B2B2B),
        .init(id: "brown", label: "Brown", hex: 0x936439),
        .init(id: "red", label: "Red", hex: 0xFF263C),
        .init(id: "orange", label: "Orange", hex: 0xFF6700),
        .init(id: "yellow", label: "Yellow", hex: 0xFF9800),
        .init(id: "green", label: "Green", hex: 0x00C972),
        .init(id: "cyan", label: "Cyan", hex: 0x00BCA6),
        .init(id: "blue", label: "Blue", hex: 0x1084FE),
        .init(id: "violet", label: "Violet", hex: 0x9159FE),
        .init(id: "magenta", label: "Magenta", hex: 0xFF309B),
        .init(id: "gray", label: "Gray", hex: 0x777777),
    ]

    public static let shapes = ["blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop"]

    public static func color(_ id: String) -> Color {
        (colors.first { $0.id == id } ?? colors[7]).color
    }
}

public enum RelativeTime {
    /// Grok-style compact age: now · 5m · 3h · 2d · Sep 3
    public static func short(_ date: Date, now: Date = .now) -> String {
        let s = max(0, now.timeIntervalSince(date))
        switch s {
        case ..<60: return "now"
        case ..<3600: return "\(Int(s / 60))m"
        case ..<86_400: return "\(Int(s / 3600))h"
        case ..<(86_400 * 7): return "\(Int(s / 86_400))d"
        default: return date.formatted(.dateTime.month(.abbreviated).day())
        }
    }

    /// Roster stamp: 3:45 AM · Yesterday · Wednesday · Sep 16
    public static func day(_ date: Date, now: Date = .now) -> String {
        let cal = Calendar.current
        if cal.isDate(date, inSameDayAs: now) { return date.formatted(date: .omitted, time: .shortened) }
        if cal.isDateInYesterday(date) { return String(localized: "Yesterday") }
        if now.timeIntervalSince(date) < 6 * 86_400 { return date.formatted(.dateTime.weekday(.wide)) }
        return date.formatted(.dateTime.month(.abbreviated).day())
    }

    /// Chat separator: Today 3:27 AM · Yesterday 5:20 PM · Sep 16 9:02 AM
    public static func separator(_ date: Date) -> String {
        let cal = Calendar.current
        let day = cal.isDateInToday(date) ? String(localized: "Today")
            : cal.isDateInYesterday(date) ? String(localized: "Yesterday")
            : date.formatted(.dateTime.month(.abbreviated).day())
        return day + " " + date.formatted(date: .omitted, time: .shortened)
    }

    public static func until(_ date: Date, now: Date = .now) -> String {
        let s = max(0, date.timeIntervalSince(now))
        let h = Int(s / 3600), m = Int(s.truncatingRemainder(dividingBy: 3600) / 60)
        if h >= 48 { return "\(h / 24)d" }
        if h > 0 { return "\(h)h \(m)m" }
        return "\(m)m"
    }
}

public enum BackendInfo {
    public static func name(_ id: String) -> String {
        switch id {
        case "claude": "Claude Code"
        case "codex": "Codex"
        case "opencode": "OpenCode"
        case "grok": "Grok Build"
        case "gemini": "Gemini CLI"
        default: "Custom agent"
        }
    }
}
