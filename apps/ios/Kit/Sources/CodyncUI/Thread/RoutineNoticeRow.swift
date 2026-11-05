import CodyncKit
import SwiftUI

/// A routine's line in the chat, which opens the routine; a failed run the user can run again has a retry button.
struct RoutineNoticeRow: View {
    let entry: Entry
    let open: () -> Void
    @Environment(BotStore.self) private var model

    var body: some View {
        HStack(spacing: 8) {
            Button(action: open) {
                Label(L10n.tr(entry.data.text ?? "Routine"), systemImage: "clock.arrow.circlepath")
                    .font(.footnote).foregroundStyle(Palette.secondary).padding(.vertical, 10)
            }
            .buttonStyle(.plain)
            if entry.data.retry == true {
                if model.answering[entry.id] != nil {
                    Spinner(size: 12)
                } else {
                    IconButton("Try again", systemImage: "arrow.clockwise") { model.retryRoutineRun(entry) }
                }
            }
        }
    }
}
