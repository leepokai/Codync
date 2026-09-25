import SwiftUI

/// Grok Bot's motion, taken from its stylesheet and motion constants so ours feels the same.
/// With Reduce Motion on, Grok drops every duration to 0; use `Motion.reduced(_:)` for that.
public enum Motion {
    /// Hover and selection fills, tabs, chips, switch knob: `background-color .12s ease`.
    public static let hover = Animation.timingCurve(0.25, 0.1, 0.25, 1, duration: 0.12)
    /// A view switched in (the transcript on a new selection): `opacity .12s`.
    public static let fade = Animation.timingCurve(0.25, 0.1, 0.25, 1, duration: 0.12)
    /// Press feedback lands almost at once: `transform 50ms` (`--cursor-duration-instant`).
    public static let press = Animation.timingCurve(0.25, 0.1, 0.25, 1, duration: 0.05)
    /// Icon and glyph swaps, the sidebar rail morph: `.2s cubic-bezier(.22,1,.36,1)`.
    public static let morph = Animation.timingCurve(0.22, 1, 0.36, 1, duration: 0.2)
    public static let morphCurve = UnitCurve.bezier(startControlPoint: UnitPoint(x: 0.22, y: 1), endControlPoint: UnitPoint(x: 0.36, y: 1))
    /// Size and layout changes (panels growing, cards resizing): the critically damped
    /// `.3s` spring Grok writes as a `linear()` curve.
    public static let layout = Animation.spring(duration: 0.3, bounce: 0)
    /// Tiles moving into place: `{type: "spring", stiffness: 1000, damping: 63}`.
    public static let tile = Animation.interpolatingSpring(mass: 1, stiffness: 1000, damping: 63)

    /// `--ui-press-scale`.
    public static let pressScale: CGFloat = 0.98

    public static func reduced(_ animation: Animation, _ reduce: Bool) -> Animation? {
        reduce ? nil : animation
    }
}

/// Grok's press: shrink to 98% in 50ms, spring back.
public struct PressScale: ButtonStyle {
    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .scaleEffect(configuration.isPressed ? Motion.pressScale : 1)
            .animation(Motion.press, value: configuration.isPressed)
    }
}
