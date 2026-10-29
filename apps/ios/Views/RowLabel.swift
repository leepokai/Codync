import CodyncKit
import SwiftUI

/// An icon and a short name, with a chevron when it leads somewhere.
struct RowLabel: View {
    let title: String
    let systemImage: String
    var tint: Color = Palette.text
    var chevron = false

    init(_ title: String, systemImage: String, tint: Color = Palette.text, chevron: Bool = false) {
        self.title = title
        self.systemImage = systemImage
        self.tint = tint
        self.chevron = chevron
    }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: systemImage)
                .font(.system(size: 17))
                .frame(width: 26)
            Text(L10n.tr(title))
            Spacer(minLength: 8)
            if chevron {
                Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
            }
        }
        .foregroundStyle(tint)
        .contentShape(Rectangle())
    }
}
