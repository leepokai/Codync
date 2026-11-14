import CodyncKit
import SwiftUI

/// An icon and a short name, with a chevron when it leads somewhere.
struct RowLabel: View {
    /// An SF Symbol sizes with the font; a custom mark gets a frame instead.
    enum Icon {
        case symbol(String)
        case image(Image)
    }

    let title: String
    let icon: Icon
    var tint: Color = Palette.text
    var chevron = false

    init(_ title: String, systemImage: String, tint: Color = Palette.text, chevron: Bool = false) {
        self.title = title
        icon = .symbol(systemImage)
        self.tint = tint
        self.chevron = chevron
    }

    init(_ title: String, image: Image, tint: Color = Palette.text, chevron: Bool = false) {
        self.title = title
        icon = .image(image)
        self.tint = tint
        self.chevron = chevron
    }

    var body: some View {
        HStack(spacing: 12) {
            iconView.frame(width: 26)
            Text(L10n.tr(title))
            Spacer(minLength: 8)
            if chevron {
                Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
            }
        }
        .foregroundStyle(tint)
        .contentShape(Rectangle())
    }

    @ViewBuilder private var iconView: some View {
        switch icon {
        case .symbol(let name): Image(systemName: name).font(.system(size: 17))
        case .image(let image): image.resizable().scaledToFit().frame(width: 20, height: 20)
        }
    }
}
