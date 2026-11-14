import SwiftUI

/// Custom images never run the generated character's sprite/timer animation.
public struct CustomIconView: View {
    let icon: CustomIcon
    let size: CGFloat
    let tint: Color

    public init(icon: CustomIcon, size: CGFloat, tint: Color = Palette.text) {
        self.icon = icon
        self.size = size
        self.tint = tint
    }

    public var body: some View {
        if let image = icon.image {
            Image(uiImage: image).resizable().renderingMode(icon.tinted ? .template : .original)
                .scaledToFit().foregroundStyle(tint).frame(width: size, height: size).accessibilityHidden(true)
        }
    }
}
