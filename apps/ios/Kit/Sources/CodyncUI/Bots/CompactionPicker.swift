import CodyncKit
import SwiftUI

/// When Codync summarizes the bot's chat ahead of time: early, near the limit, off, or a size its window holds.
struct CompactionPicker: View {
    @Binding var selection: CompactionThreshold
    /// Nil until the agent has told its window.
    let range: TokenRange?
    @State private var typing = false
    @State private var typed = ""

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
        .codyncDialog("Custom size", isPresented: $typing, message: "Thousands of tokens, rounded to 10K, up to what the model holds.",
                      field: DialogField(range.map { "\($0.min / 1000)–\($0.max / 1000)" } ?? "", text: $typed, keyboard: .numbersAndPunctuation)) {
            let tokens = TokenRange.typed(typed)
            return [DialogAction("Save", enabled: tokens != nil) {
                if let range, let tokens { selection = .tokens(range.fit(tokens)) }
            }]
        }
    }

    private var items: [MenuItem] {
        let rows = Self.presets.map { choice, title in MenuItem(title, selected: selection == choice) { selection = choice } }
        guard let range else {
            return rows + [MenuItem("Sizes show after the bot's next reply", divider: true, enabled: false) {}]
        }
        let picks = range.presets.map { tokens in
            MenuItem(Self.size(tokens), selected: selection == .tokens(tokens), verbatim: true) { selection = .tokens(tokens) }
        }
        var own = false
        if case .tokens(let tokens) = selection { own = !range.presets.contains(tokens) }
        let custom = MenuItem("Custom size…", selected: own, divider: !picks.isEmpty) {
            if case .tokens(let tokens) = selection { typed = "\(tokens / 1000)" } else { typed = "" }
            typing = true
        }
        return rows + [MenuItem("Set a size", divider: true, submenu: picks + [custom])]
    }

    private var label: String {
        if case .tokens(let tokens) = selection { return Self.size(tokens) }
        return L10n.tr((Self.presets.first { $0.0 == selection } ?? Self.presets[0]).1)
    }

    /// "120K".
    static func size(_ tokens: Int) -> String { "\(tokens / 1000)K" }
}
