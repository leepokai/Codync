import CodyncKit
import SwiftUI

/// Find in the open conversation: a bar floating over the top of the chat; the message holding the match shown is
/// highlighted in the chat itself. Return goes to the next match.
struct FindBar: View {
    @Binding var find: ConversationFind
    let matches: [ConversationFind.Match]
    let step: (_ forward: Bool) -> Void
    let close: () -> Void

    var body: some View {
        HStack(spacing: 4) {
            SearchField(L10n.tr("Find in conversation"), text: Binding(get: { find.query }, set: { find.type($0) }), autoFocus: true)
                .onSubmit { step(true) }
                .submitLabel(.search)
            let label = find.label(matches)
            if !label.isEmpty {
                Text(label)
                    .font(.footnote.monospacedDigit())
                    .foregroundStyle(matches.isEmpty ? Palette.attention : Palette.secondary)
                    .lineLimit(1)
                    .fixedSize()
                    .padding(.horizontal, 4)
            }
            IconButton("Previous match", systemImage: "chevron.up") { step(false) }
                .disabled(matches.isEmpty)
            IconButton("Next match", systemImage: "chevron.down") { step(true) }
                .disabled(matches.isEmpty)
            IconButton("Close", systemImage: "xmark", action: close)
        }
        .padding(.leading, 6)
        .padding(.trailing, 4)
        .padding(.vertical, 4)
        .glass(in: Capsule())
        .padding(.horizontal, 12)
    }
}
