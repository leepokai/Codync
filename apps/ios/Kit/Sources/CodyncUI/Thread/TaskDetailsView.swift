import CodyncKit
import SwiftUI

/// One item's task, read only: its status and time, and everything the task did, kept current while it works.
struct TaskDetailsView: View {
    let botId: String
    /// The item as its row had it; the bot's own copy is shown while it carries one.
    let item: WorkItem
    @Environment(BotStore.self) private var model
    /// The task's transcript as the computer has it; nil while it loads.
    @State private var fetched: [Entry]?
    @State private var error: String?
    @State private var attempt = 0
    @State private var confirmingStop = false

    /// What reads the transcript again: Try again, a task taking the item, and the item ending.
    private struct Load: Hashable {
        var attempt: Int
        var task: String?
        var live: Bool
    }

    var body: some View {
        let item = model.bots[botId]?.work.first { $0.id == self.item.id } ?? self.item
        let live = item.status.isLive
        let entries = (fetched ?? []).mergingTaskEntries(item.task.flatMap { model.taskEntries[$0] } ?? [])
        let turns = Dictionary(grouping: entries, by: \.turn).sorted { $0.key < $1.key }
        let running = Entry.runningTurn(entries, working: live)
        VStack(spacing: 0) {
            ModalHeader {
                Text(item.title)
            } trailing: {
                if live { IconButton("Stop", systemImage: "stop.fill") { confirmingStop = true } }
            }
            HStack(spacing: 6) {
                Text(L10n.tr(item.status.word)).font(.footnote).foregroundStyle(Palette.secondary)
                WorkElapsed(item: item)
                if let branch = item.worktree?.branch {
                    Label(branch, systemImage: "arrow.triangle.branch")
                        .font(.footnote)
                        .foregroundStyle(Palette.tertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .padding(.leading, 6)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 20)
            .padding(.bottom, 8)
            .accessibilityElement(children: .combine)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 22) {
                    state(empty: entries.isEmpty).frame(maxWidth: .infinity)
                    ForEach(turns, id: \.key) { turn, entries in
                        CardSection {
                            ForEach(entries) { entry in
                                if entry.isToldToTask {
                                    Text(verbatim: "“\(entry.data.text ?? "")”")
                                        .font(.subheadline)
                                        .foregroundStyle(Palette.secondary)
                                        .textSelection(.enabled)
                                } else {
                                    TraceRow(entry: entry, unfolded: turn == running)
                                }
                            }
                        }
                    }
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .defaultScrollAnchor(.bottom)
        }
        .background(Palette.background)
        .task(id: Load(attempt: attempt, task: item.task, live: live)) { await load(item.task) }
        .stopWorkDialog(isPresented: $confirmingStop) { model.stopWork(botId, item: item.id) }
        .tracked(.screenViewed(.task))
    }

    @ViewBuilder private func state(empty: Bool) -> some View {
        if let error {
            VStack(spacing: 8) {
                Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger)
                Button(L10n.tr("Try again")) { attempt += 1 }
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(Palette.text)
                    .buttonStyle(PressScale())
            }
        } else if fetched == nil {
            Spinner(size: 14).accessibilityLabel(L10n.tr("Loading"))
        } else if empty {
            Text(L10n.tr("Nothing yet.")).font(.footnote).foregroundStyle(Palette.tertiary)
        }
    }

    private func load(_ task: String?) async {
        error = nil
        guard let task else {
            fetched = []
            return
        }
        do {
            fetched = try await model.fetchTaskEntries(botId, task: task)
        } catch is CancellationError {
        } catch {
            self.error = error.localizedDescription
        }
    }
}
