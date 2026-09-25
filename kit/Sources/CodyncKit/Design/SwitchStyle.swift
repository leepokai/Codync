import SwiftUI

/// Codync's own on/off switch: label on the left, a flat capsule track on the right.
public struct CodyncSwitch: ToggleStyle {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        Button {
            withAnimation(Motion.reduced(Motion.hover, reduceMotion)) { configuration.isOn.toggle() }
        } label: {
            HStack(spacing: 12) {
                configuration.label
                Spacer(minLength: 0)
                Capsule()
                    .fill(configuration.isOn ? Palette.switchOn : Palette.bubbleUser)
                    .frame(width: 34, height: 20)
                    .overlay(alignment: configuration.isOn ? .trailing : .leading) {
                        Circle().fill(.white).padding(2)
                    }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityRepresentation { Toggle(isOn: configuration.$isOn) { configuration.label } }
    }
}

public extension ToggleStyle where Self == CodyncSwitch {
    static var codync: CodyncSwitch { CodyncSwitch() }
}
