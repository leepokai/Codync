import CodyncKit
import SwiftUI

/// A turn that stopped half done: said so, with one tap to go on (a few times in a row, then it
/// asks for words) or to end the task there. Ending it keeps the messages.
struct PausedTurnRow: View {
    let entry: Entry
    let paused: PausedTurn
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label(L10n.tr(paused.verdict == .truncated ? "Answer cut short" : "Paused halfway"),
                  systemImage: "pause.circle.fill")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(Palette.warning)
            Text(entry.data.text ?? "")
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
            if paused.status == .waiting {
                if !paused.canContinue {
                    Text(L10n.tr("Say what remains in a message."))
                        .font(.caption)
                        .foregroundStyle(Palette.tertiary)
                }
                HStack(spacing: 8) {
                    if paused.canContinue {
                        Button(L10n.tr("Continue")) { model.continueTurn(entry) }
                            .buttonStyle(PrimaryButtonStyle())
                    }
                    Button(L10n.tr("Cancel task")) { model.cancelTask(entry) }
                        .buttonStyle(SecondaryButtonStyle())
                }
                .disabled(model.answering[entry.id] != nil)
            } else {
                Text(L10n.tr(paused.status == .closed ? "Cancelled" : "Continued"))
                    .font(.caption)
                    .foregroundStyle(Palette.tertiary)
            }
        }
        .padding(12)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 14))
        .padding(.trailing, 40)
    }
}
