import CodyncKit
import SwiftUI

/// A missing custom asset also falls back to a still frame, never the timer/sprite loop.
struct CustomActivityIcon: View {
    let attributes: BotActivityAttributes
    let size: CGFloat

    var body: some View {
        if let filename = attributes.currentCustomIconName, let image = ActivityIconCache.image(filename: filename, context: attributes.iconContext) {
            Image(uiImage: image).resizable().renderingMode(attributes.currentCustomIconTinted ? .template : .original)
                .scaledToFit().foregroundStyle(AvatarPalette.color(attributes.avatarColor))
                .frame(width: size, height: size)
        } else {
            CharacterAvatar(shape: attributes.avatarShape, color: attributes.avatarColor, size: size, still: true)
        }
    }
}
