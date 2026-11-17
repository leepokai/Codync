import CodyncKit
import SwiftUI

/// A bot's work in the background, directly above the composer: how much goes on (the one item: what it does,
/// since when) and, with one item live, Stop. Tapping the words opens the bot's details.
struct WorkCapsule: View {
    let botId: String
    let work: WorkSections
    let open: () -> Void
    @Environment(BotStore.self) private var model
    /// The item Stop asked about: the one it stops, whatever started since.
    @State private var stopping: String?
    @State private var confirmingStop = false

    private var mark: WorkItem.Status {
        !work.needsYou.isEmpty ? .needsYou : work.working.isEmpty ? .waiting : .working
    }

    var body: some View {
        HStack(spacing: 4) {
            Button(action: open) {
                HStack(spacing: 8) {
                    WorkMark(status: mark)
                    Text(work.capsuleLabel())
                        .font(.subheadline)
                        .foregroundStyle(Palette.text)
                        .lineLimit(1)
                    if let since = work.solo?.startedAt {
                        Text(Date(milliseconds: since), style: .timer)
                            .font(.footnote.monospacedDigit())
                            .foregroundStyle(Palette.tertiary)
                    }
                }
                // As wide as the composer under it: the words start at the left, Stop sits at the right.
                .frame(maxWidth: .infinity, minHeight: 36, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(PressScale())
            .accessibilityHint(L10n.tr("Details"))
            if let item = work.stoppable {
                IconButton("Stop", systemImage: "stop.fill") {
                    stopping = item.id
                    confirmingStop = true
                }
            }
        }
        .padding(.leading, 14)
        .padding(.trailing, work.stoppable == nil ? 14 : 2)
        .glass(in: Capsule())
        .padding(.horizontal, 16)
        .stopWorkDialog(isPresented: $confirmingStop) {
            if let stopping { model.stopWork(botId, item: stopping) }
        }
    }
}

extension View {
    /// Asks once before background work is stopped.
    func stopWorkDialog(isPresented: Binding<Bool>, stop: @escaping () -> Void) -> some View {
        codyncDialog("Stop the background work?", isPresented: isPresented, message: "What it already changed stays.") {
            [DialogAction("Stop", destructive: true, action: stop)]
        }
    }
}
