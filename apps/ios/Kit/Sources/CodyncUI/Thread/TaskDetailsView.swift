import CodyncKit
import SwiftUI

/// What the capsule opens: the one task's details, or with several a list first, where a row opens that task.
struct TasksSheet: View {
    let botId: String
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var opened: String?

    init(botId: String, tasks: [BotTask]) {
        self.botId = botId
        _opened = State(initialValue: tasks.count == 1 ? tasks[0].id : nil)
    }

    private func open(_ task: String?) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { opened = task }
    }

    var body: some View {
        let tasks = model.bots[botId]?.tasks ?? []
        Group {
            if let opened {
                TaskDetailsView(botId: botId, taskId: opened, back: tasks.count > 1 ? { open(nil) } : nil)
                    .id(opened)
                    .transition(.opacity)
            } else {
                list(tasks).transition(.opacity)
            }
        }
        .background(Palette.background)
        // The list has nothing left to show once the last task ended; an open task stays, to read how it ended.
        .onChange(of: tasks.isEmpty) { if tasks.isEmpty, opened == nil { dismiss() } }
    }

    private func list(_ tasks: [BotTask]) -> some View {
        VStack(spacing: 0) {
            ModalHeader("In the background")
            ScrollView {
                CardSection {
                    ForEach(tasks) { task in
                        Button { open(task.id) } label: {
                            HStack(spacing: 8) {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(task.title).font(.body).foregroundStyle(Palette.text).lineLimit(1)
                                    Text(L10n.tr(task.status.word)).font(.footnote).foregroundStyle(Palette.secondary)
                                }
                                Spacer(minLength: 8)
                                TaskElapsed(task: task)
                                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(20)
            }
        }
    }
}

/// One background task, read only: its status and time, and everything it did, kept current while it works.
struct TaskDetailsView: View {
    let botId: String
    let taskId: String
    /// Back to the list it was opened from; nil when the capsule opened it directly.
    var back: (() -> Void)?
    @Environment(BotStore.self) private var model
    /// The task and its transcript as the computer has them; nil while they load.
    @State private var fetched: Fetched?
    @State private var error: String?
    @State private var attempt = 0
    @State private var confirmingStop = false

    private struct Fetched {
        var task: BotTask
        var entries: [Entry]
    }

    /// What reads the task again: Try again, and the task ending (its bot no longer carries it).
    private struct Load: Hashable {
        var attempt: Int
        var live: Bool
    }

    var body: some View {
        let live = model.bots[botId]?.tasks.first { $0.id == taskId }
        let task = live ?? fetched?.task
        let entries = (fetched?.entries ?? []).mergingTaskEntries(model.taskEntries[taskId] ?? [])
        let turns = Dictionary(grouping: entries, by: \.turn).sorted { $0.key < $1.key }
        let running = Entry.runningTurn(entries, working: live != nil)
        VStack(spacing: 0) {
            ModalHeader {
                HStack(spacing: 4) {
                    if let back { IconButton("Back", systemImage: "chevron.left", action: back).padding(.leading, -12) }
                    Text(task?.title ?? "")
                }
            } trailing: {
                if live != nil { IconButton("Stop", systemImage: "stop.fill") { confirmingStop = true } }
            }
            if let task {
                HStack(spacing: 6) {
                    Text(L10n.tr(task.status.word)).font(.footnote).foregroundStyle(Palette.secondary)
                    TaskElapsed(task: task)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 20)
                .padding(.bottom, 8)
                .accessibilityElement(children: .combine)
            }
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
        .task(id: Load(attempt: attempt, live: live != nil)) { await load() }
        .stopTaskDialog(isPresented: $confirmingStop) { model.stopTask(botId, task: taskId) }
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

    private func load() async {
        error = nil
        do {
            let read = try await model.fetchTask(botId, task: taskId)
            fetched = Fetched(task: read.task, entries: read.entries)
        } catch is CancellationError {
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// How long a task has run: counting while it works, fixed once it ended.
private struct TaskElapsed: View {
    let task: BotTask

    var body: some View {
        Group {
            if let ended = task.endedAt {
                Text(Duration.milliseconds(max(0, ended - task.startedAt)).formatted(.time(pattern: .minuteSecond)))
            } else if task.status.isLive {
                Text(Date(milliseconds: task.startedAt), style: .timer)
            }
        }
        .font(.footnote.monospacedDigit())
        .foregroundStyle(Palette.tertiary)
    }
}
