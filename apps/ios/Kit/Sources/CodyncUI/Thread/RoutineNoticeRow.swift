import CodyncKit
import SwiftUI

/// A routine's line in the chat, which opens the routine. A failed run that still offers a retry
/// can be tried again or ended there; the messages stay.
struct RoutineNoticeRow: View {
    let entry: Entry
    let open: () -> Void
    @Environment(BotStore.self) private var model

    private var failed: Bool { entry.data.source == "scheduled_task" && entry.data.style != nil }

    var body: some View {
        HStack(spacing: 8) {
            Button(action: open) {
                Label(entry.data.text ?? "Routine", systemImage: "clock.arrow.circlepath")
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
            if failed && entry.data.cancelled != true {
                IconButton("Cancel task", systemImage: "xmark") { model.cancelRoutineRun(entry) }
            }
            if entry.data.cancelled == true {
                Text("Cancelled").font(.caption).foregroundStyle(Palette.tertiary)
            }
        }
    }
}
