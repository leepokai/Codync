import CodyncKit
import SwiftUI

/// What a bot does, on its details: Threads (the ones that run now, and the ones not in use) and the event
/// box (what waits for one) above `between` (what it does later and keeps running), Recent below. A row opens its
/// task's thread (`openThread`); one that needs the user goes to its card in the chat. `plain` (the view of all bots' work):
/// only the rows of what runs and what waits, in one card.
struct WorkTimeline<Between: View>: View {
    let bot: Bot
    /// Shows the bot's chat at the card a task waits on; nil where the details aren't over that chat.
    let openCard: ((WorkTask) -> Void)?
    /// Shows the thread that has or had a task.
    let openThread: (WorkTask) -> Void
    var plain = false
    @ViewBuilder let between: Between
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The task Stop or Cut in asks about.
    @State private var asked: WorkTask?
    @State private var confirmingStop = false
    @State private var pickingCutIn = false

    var body: some View {
        let work = WorkSections(bot.work)
        VStack(alignment: .leading, spacing: 28) {
            if plain {
                CardSection {
                    rows(work.live, in: work)
                    rows(work.waiting, in: work, numbered: true)
                }
            } else if !work.isIdle {
                CardSection("Threads") {
                    rows(work.live, in: work)
                    ForEach(0..<work.free(maxThreads: bot.maxThreads), id: \.self) { _ in FreeRow() }
                } accessory: {
                    count(work.threadsCount(maxThreads: bot.maxThreads))
                }
                .transition(.opacity)
            }
            if !plain, !work.waiting.isEmpty {
                CardSection("Event box") { rows(work.waiting, in: work, numbered: true) } accessory: { count("\(work.waiting.count)") }
                    .transition(.opacity)
            }
            between
            if !plain, !work.recent.isEmpty {
                CardSection("Recent") { rows(work.recent, in: work) }.transition(.opacity)
            }
        }
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: bot.work.map(\.id))
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: bot.work.map(\.status))
        .stopWorkDialog(isPresented: $confirmingStop) {
            if let asked { model.stopWork(bot.id, task: asked.id) }
        }
        .codyncDialog(L10n.fill(L10n.tr("Let \"{title}\" cut in"), ["title": asked?.title ?? ""]), isPresented: $pickingCutIn,
                      message: "Every thread is busy. Pick the task that gives way: its thread closes and the task starts again later.") {
            work.givingWay.map { giving in
                DialogAction(cutInChoice(giving)) {
                    if let asked { model.workCutIn(bot.id, task: asked.id, insteadOf: giving.id) }
                }
            }
        }
    }

    /// `numbered`: the order the box is taken in, in the mark's place.
    private func rows(_ tasks: [WorkTask], in work: WorkSections, numbered: Bool = false) -> some View {
        ForEach(Array(tasks.enumerated()), id: \.element.id) { index, task in
            let card = task.status == .needsYou ? model.chat(bot.id).card(for: task) : nil
            WorkRow(task: task, number: numbered ? index + 1 : nil, subtitle: card?.ask ?? task.line(),
                    actions: work.actions(for: task, maxThreads: bot.maxThreads),
                    open: opener(task, card: card)) { run($0, on: task) }
        }
    }

    /// A heading's count: how many run of the most allowed, how many wait.
    private func count(_ text: String) -> some View {
        Text(verbatim: text).font(.footnote.monospacedDigit()).foregroundStyle(Palette.tertiary).padding(.trailing, 2)
    }

    private func opener(_ task: WorkTask, card: Entry?) -> (() -> Void)? {
        if task.status == .needsYou, let openCard { return { openCard(task) } }
        return task.thread == nil ? nil : { openThread(task) }
    }

    private func run(_ action: WorkAction, on task: WorkTask) {
        switch action {
        case .stop:
            asked = task
            confirmingStop = true
        case .cutIn:
            asked = task
            pickingCutIn = true
        case .first: model.workFirst(bot.id, task: task.id)
        case .drop: model.dropWork(bot.id, task: task.id)
        case .retry: model.retryWork(bot.id, task: task.id)
        }
    }

    /// A working task in the cut-in dialog: its title, what it does, how long.
    private func cutInChoice(_ task: WorkTask) -> String {
        let elapsed = task.startedAt.map { Duration.seconds(max(0, Date.now.timeIntervalSince(Date(milliseconds: $0)))).formatted(.time(pattern: .minuteSecond)) }
        return [task.title, task.status == .needsYou || !task.activity.isEmpty ? task.what() : nil, elapsed]
            .compactMap(\.self).joined(separator: " · ")
    }
}

extension WorkTimeline where Between == EmptyView {
    /// A bot's rows in the view of all bots' work.
    init(plain bot: Bot, openCard: @escaping (WorkTask) -> Void, openThread: @escaping (WorkTask) -> Void) {
        self.init(bot: bot, openCard: openCard, openThread: openThread, plain: true) {}
    }
}

/// One task: its mark, title, what goes on and where its thread works, its time, and the action that matters most; the
/// rest is in its touch-and-hold menu.
private struct WorkRow: View {
    let task: WorkTask
    /// Its place in the event box, shown where the mark is.
    var number: Int?
    let subtitle: String
    let actions: [WorkAction]
    let open: (() -> Void)?
    let run: (WorkAction) -> Void
    @Environment(BotStore.self) private var model

    var body: some View {
        let offline = model.isOffline
        HStack(spacing: 8) {
            if let open { Button(action: open) { line }.buttonStyle(.plain) } else { line }
            if let primary = actions.first { button(primary).disabled(offline) }
        }
        .contextActions {
            actions.map { action in
                MenuItem(action.title, icon: action.symbol, destructive: action == .stop || action == .drop, enabled: !offline) { run(action) }
            }
        }
    }

    private var line: some View {
        HStack(spacing: 10) {
            if let number {
                Text(verbatim: "\(number)").font(.footnote.monospacedDigit()).foregroundStyle(Palette.tertiary).frame(width: 16)
            } else {
                WorkMark(status: task.status)
            }
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    source
                    Text(task.title).foregroundStyle(Palette.text).lineLimit(1)
                }
                Text(subtitle)
                    .font(.footnote)
                    .foregroundStyle(task.status == .failed || task.status == .stopped ? Palette.danger : Palette.secondary)
                    .lineLimit(2)
                if let place = task.whereLine(home: model.hello?.home ?? "") { WhereLabel(place: place) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            WorkElapsed(task: task)
            if open != nil, task.status.hasEnded {
                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
            }
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    /// Where the task came from, before its title: a routine's symbol, or the avatar of the bot that asked. Nothing
    /// for the bot's own, or for a bot that isn't on this computer.
    @ViewBuilder private var source: some View {
        switch task.source?.kind {
        case .routine:
            Image(systemName: "clock.arrow.circlepath").font(.footnote).foregroundStyle(Palette.secondary)
                .accessibilityLabel(L10n.tr("Routine"))
        case .request:
            if let asking = task.source?.id.flatMap({ model.bots[$0] }) {
                BotAvatar(bot: asking, members: model.members(of: asking), size: 16, animated: false)
                    .accessibilityLabel(asking.name)
            }
        case nil: EmptyView()
        }
    }

    /// Icon-only, but Cut in: no icon says it.
    @ViewBuilder private func button(_ action: WorkAction) -> some View {
        if action == .cutIn {
            Button { run(action) } label: {
                Text(L10n.tr(action.title)).font(.footnote.weight(.medium)).foregroundStyle(Palette.text).outlinedPill()
            }
            .buttonStyle(PressScale())
        } else {
            IconButton(action.title, systemImage: action.symbol) { run(action) }
                // Its target in a row of the card's height, the glyph near the card's edge.
                .padding(.vertical, -6)
                .padding(.trailing, -8)
        }
    }
}

/// A thread not in use: the ceiling, visible.
private struct FreeRow: View {
    var body: some View {
        HStack(spacing: 10) {
            WorkMark(status: .waiting).accessibilityHidden(true)
            Text(L10n.tr("Free")).foregroundStyle(Palette.tertiary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A task's status as a mark: attention, a moving dot, a faint ring (waiting), a ring (done), the danger mark.
struct WorkMark: View {
    let status: WorkTask.Status

    var body: some View {
        Group {
            switch status {
            case .needsYou: ThinkingOrb(state: .listening, size: 14, color: Palette.warning)
            case .working: ThinkingOrb(size: 13, color: Palette.secondary)
            case .waiting: Circle().strokeBorder(Palette.tertiary, lineWidth: 1.5).frame(width: 10, height: 10)
            case .done: Circle().strokeBorder(Palette.secondary, lineWidth: 1.5).frame(width: 10, height: 10)
            case .failed, .stopped: Circle().fill(Palette.danger).frame(width: 8, height: 8)
            }
        }
        .frame(width: 16, height: 16)
        .accessibilityLabel(L10n.tr(status.word))
    }
}

/// Where a task's thread works, one quiet line: the folder symbol and the folder, or the branch symbol and the
/// branch of its own worktree. A long one keeps both ends.
struct WhereLabel: View {
    let place: WhereLine

    var body: some View {
        Label(place.text, systemImage: place.kind.symbol)
            .font(.footnote)
            .foregroundStyle(Palette.tertiary)
            .lineLimit(1)
            .truncationMode(.middle)
    }
}

/// How long a thread has had a task: counting while it works, fixed once it ended.
struct WorkElapsed: View {
    let task: WorkTask

    var body: some View {
        Group {
            if let taken = task.timeTaken {
                Text(taken.formatted(.time(pattern: .minuteSecond)))
            } else if task.status.isLive, let started = task.startedAt {
                Text(Date(milliseconds: started), style: .timer)
            }
        }
        .font(.footnote.monospacedDigit())
        .foregroundStyle(Palette.tertiary)
    }
}
