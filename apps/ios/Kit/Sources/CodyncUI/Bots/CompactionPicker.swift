import CodyncKit
import SwiftUI

/// When Codync summarizes the bot's chat ahead of time: early, near the limit, off, or a size its window holds.
struct CompactionPicker: View {
    @Binding var selection: CompactionThreshold
    /// Nil until the agent has told its window.
    let range: TokenRange?

    private static let presets: [(CompactionThreshold, String)] =
        [(.early, "Early (default)"), (.nearLimit, "Near the limit"), (.off, "Only when full")]

    var body: some View {
        ValueRow("Summarize long chats",
                 detail: "When Codync summarizes this chat so it can go on. If the chat fills up first, the agent summarizes it itself.") {
            DropdownMenu { items } label: {
                HStack(spacing: 6) {
                    Text(verbatim: label).lineLimit(1)
                    Image(systemName: "chevron.down").font(.caption2.weight(.semibold)).foregroundStyle(Palette.secondary)
                }
                .foregroundStyle(Palette.text)
                .outlinedPill()
            }
        }
    }

    private var items: [MenuItem] {
        let rows = Self.presets.map { choice, title in MenuItem(title, selected: selection == choice) { selection = choice } }
        guard let sizes = range?.sizes, !sizes.isEmpty else {
            return rows + [MenuItem("Sizes show after the bot's next reply", divider: true, enabled: false) {}]
        }
        let picks = sizes.map { tokens in
            MenuItem(Self.size(tokens), selected: selection == .tokens(tokens), verbatim: true) { selection = .tokens(tokens) }
        }
        return rows + [MenuItem("Set a size", divider: true, submenu: picks)]
    }

    private var label: String {
        if case .tokens(let tokens) = selection { return Self.size(tokens) }
        return L10n.tr((Self.presets.first { $0.0 == selection } ?? Self.presets[0]).1)
    }

    /// "120K".
    static func size(_ tokens: Int) -> String { "\(tokens / 1000)K" }
}
