import CodyncKit
import SwiftUI

/// Plain icon button that lights up on hover and on press, like toolbar buttons.
struct IconButton: View {
    let title: String
    let systemImage: String
    var selected = false
    let action: () -> Void

    init(_ title: String, systemImage: String, selected: Bool = false, action: @escaping () -> Void) {
        self.title = title
        self.systemImage = systemImage
        self.selected = selected
        self.action = action
    }

    var body: some View {
        Button(title, systemImage: systemImage, action: action)
            .labelStyle(.iconOnly)
            .buttonStyle(IconButtonStyle(selected: selected))
            .help(title)
    }
}

struct IconButtonStyle: ButtonStyle {
    var selected = false
    var size: CGFloat = 28
    @State private var hovering = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: size / 2, weight: .medium))
            .foregroundStyle(selected ? Palette.onAccent : hovering ? Palette.text : Palette.secondary)
            .frame(width: size, height: size)
            .background(
                selected ? Palette.accentFill : Palette.text.opacity(configuration.isPressed ? 0.14 : hovering ? 0.08 : 0),
                in: RoundedRectangle(cornerRadius: size * 0.28, style: .continuous)
            )
            .scaleEffect(configuration.isPressed ? Motion.pressScale : 1)
            .animation(Motion.press, value: configuration.isPressed)
            .animation(Motion.hover, value: hovering)
            .animation(Motion.hover, value: selected)
            .contentShape(Rectangle())
            .onHover { hovering = $0 }
    }
}
