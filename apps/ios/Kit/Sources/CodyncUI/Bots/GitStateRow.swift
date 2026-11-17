import CodyncKit
import SwiftUI

/// Under a bot's folder in Settings: what the folder is to git, and the manual's page on working folders and git
/// (what agents working at once do with and without it, and how to install it).
struct GitStateRow: View {
    let state: Bot.GitState
    @Environment(\.openURL) private var openURL

    private static let manual = URL(string: "https://codync.dev/docs/working-folders")

    var body: some View {
        HStack(spacing: 8) {
            Text(L10n.tr(state.line))
                .font(.caption)
                .foregroundStyle(Palette.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            IconButton("About working folders and git", systemImage: "info.circle") {
                guard let manual = Self.manual else { return }
                Analytics.shared.capture(.linkOpened(.docs))
                openURL(manual)
            }
            // Its target in a row of the line's height, the glyph near the card's edge.
            .padding(.vertical, -8)
            .padding(.trailing, -8)
        }
        .accessibilityIdentifier("git-state")
    }
}
