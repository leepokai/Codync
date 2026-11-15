import CodyncKit
import SwiftUI

/// A bot's background tasks, directly above the composer: what goes on, since when, and Stop. Tapping the words
/// opens the details (`TasksSheet`).
struct TaskCapsule: View {
    let botId: String
    let tasks: [BotTask]
    let open: () -> Void
    @Environment(BotStore.self) private var model
    @State private var confirmingStop = false

    var body: some View {
        HStack(spacing: 4) {
            Button(action: open) {
                HStack(spacing: 8) {
                    Spinner(size: 14)
                    Text(tasks.capsuleLabel())
                        .font(.subheadline)
                        .foregroundStyle(Palette.text)
                        .lineLimit(1)
                    if let since = tasks.since {
                        Text(Date(milliseconds: since), style: .timer)
                            .font(.footnote.monospacedDigit())
                            .foregroundStyle(Palette.tertiary)
                    }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(PressScale())
            .accessibilityHint(L10n.tr("Details"))
            IconButton("Stop", systemImage: "stop.fill") { confirmingStop = true }
        }
        .padding(.leading, 14)
        .padding(.trailing, 2)
        .glass(in: Capsule())
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 16)
        .stopTaskDialog(isPresented: $confirmingStop) {
            model.stopTask(botId, task: tasks.count == 1 ? tasks[0].id : nil)
        }
    }
}

extension View {
    /// Asks once before background work is stopped.
    func stopTaskDialog(isPresented: Binding<Bool>, stop: @escaping () -> Void) -> some View {
        codyncDialog("Stop the background work?", isPresented: isPresented, message: "What it already changed stays.") {
            [DialogAction("Stop", destructive: true, action: stop)]
        }
    }
}
