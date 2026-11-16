import CodyncKit
import SwiftUI

/// What a bot does, on its details: Threads (the sessions that run now, and the places not in use) and the event
/// box (what waits for one) above `between` (what it does later and keeps running), Recent below. A row opens its
/// task's thread (`openThread`); one that needs the user goes to its card in the chat. `plain` (the view of all bots' work):
/// only the rows of what runs and what waits, in one card.
struct WorkTimeline<Between: View>: View {
    let bot: Bot
    /// Shows the bot's chat at the card an item waits on; nil where the details aren't over that chat.
    let openCard: ((WorkItem) -> Void)?
    /// Shows the thread of an item a task has or had.
    let openThread: (WorkItem) -> Void
    var plain = false
    @ViewBuilder let between: Between
    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// The item Stop or Cut in asks about.
    @State private var asked: WorkItem?
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
                    ForEach(0..<work.free(maxAgents: bot.maxAgents), id: \.self) { _ in FreeRow() }
                } accessory: {
                    count(work.threadsCount(maxAgents: bot.maxAgents))
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
            if let asked { model.stopWork(bot.id, item: asked.id) }
        }
        .codyncDialog(L10n.fill(L10n.tr("Let \"{title}\" cut in"), ["title": asked?.title ?? ""]), isPresented: $pickingCutIn,
                      message: "Every place is taken. Pick what gives way: its session closes and it starts again later.") {
            work.givingWay.map { giving in
                DialogAction(cutInChoice(giving)) {
                    if let asked { model.workCutIn(bot.id, item: asked.id, insteadOf: giving.id) }
                }
            }
        }
    }

    /// `numbered`: the order the box is taken in, in the mark's place.
    private func rows(_ items: [WorkItem], in work: WorkSections, numbered: Bool = false) -> some View {
        ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
            let card = item.status == .needsYou ? model.chat(bot.id).card(for: item) : nil
            WorkRow(item: item, number: numbered ? index + 1 : nil, subtitle: card?.ask ?? item.line(),
                    actions: work.actions(for: item, maxAgents: bot.maxAgents),
                    open: opener(item, card: card)) { run($0, on: item) }
        }
    }

    /// A heading's count: how many run of the most allowed, how many wait.
    private func count(_ text: String) -> some View {
        Text(verbatim: text).font(.footnote.monospacedDigit()).foregroundStyle(Palette.tertiary).padding(.trailing, 2)
    }

    private func opener(_ item: WorkItem, card: Entry?) -> (() -> Void)? {
        if item.status == .needsYou, let openCard { return { openCard(item) } }
        return item.task == nil ? nil : { openThread(item) }
    }

    private func run(_ action: WorkAction, on item: WorkItem) {
        switch action {
        case .stop:
            asked = item
            confirmingStop = true
        case .cutIn:
            asked = item
            pickingCutIn = true
        case .first: model.workFirst(bot.id, item: item.id)
        case .drop: model.dropWork(bot.id, item: item.id)
        case .retry: model.retryWork(bot.id, item: item.id)
        }
    }

    /// A working item in the cut-in dialog: its title, what it does, how long.
    private func cutInChoice(_ item: WorkItem) -> String {
        let elapsed = item.startedAt.map { Duration.seconds(max(0, Date.now.timeIntervalSince(Date(milliseconds: $0)))).formatted(.time(pattern: .minuteSecond)) }
        return [item.title, item.status == .needsYou || !item.activity.isEmpty ? item.what() : nil, elapsed]
            .compactMap(\.self).joined(separator: " · ")
    }
}

extension WorkTimeline where Between == EmptyView {
    /// A bot's rows in the view of all bots' work.
    init(plain bot: Bot, openCard: @escaping (WorkItem) -> Void, openThread: @escaping (WorkItem) -> Void) {
        self.init(bot: bot, openCard: openCard, openThread: openThread, plain: true) {}
    }
}

/// One item: its mark, title, what goes on and where its task works, its time, and the action that matters most; the
/// rest is in its touch-and-hold menu.
private struct WorkRow: View {
    let item: WorkItem
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
                WorkMark(status: item.status)
            }
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    source
                    Text(item.title).foregroundStyle(Palette.text).lineLimit(1)
                }
                Text(subtitle)
                    .font(.footnote)
                    .foregroundStyle(item.status == .failed || item.status == .stopped ? Palette.danger : Palette.secondary)
                    .lineLimit(2)
                if let place = item.whereLine(home: model.hello?.home ?? "") { WhereLabel(place: place) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            WorkElapsed(item: item)
            if open != nil, item.status.hasEnded {
                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
            }
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    /// Where the item came from, before its title: a routine's symbol, or the avatar of the bot that asked. Nothing
    /// for the bot's own, or for a bot that isn't on this computer.
    @ViewBuilder private var source: some View {
        switch item.source?.kind {
        case .routine:
            Image(systemName: "clock.arrow.circlepath").font(.footnote).foregroundStyle(Palette.secondary)
                .accessibilityLabel(L10n.tr("Routine"))
        case .request:
            if let asking = item.source?.id.flatMap({ model.bots[$0] }) {
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

/// A place no task has: the ceiling, visible.
private struct FreeRow: View {
    var body: some View {
        HStack(spacing: 10) {
            WorkMark(status: .waiting).accessibilityHidden(true)
            Text(L10n.tr("Free")).foregroundStyle(Palette.tertiary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// An item's status as a mark: attention, a moving dot, a faint ring (waiting), a ring (done), the danger mark.
struct WorkMark: View {
    let status: WorkItem.Status

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

/// Where an item's task works, one quiet line: the folder symbol and the folder, or the branch symbol and the
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

/// How long a task has had an item: counting while it works, fixed once it ended.
struct WorkElapsed: View {
    let item: WorkItem

    var body: some View {
        Group {
            if let taken = item.timeTaken {
                Text(taken.formatted(.time(pattern: .minuteSecond)))
            } else if item.status.isLive, let started = item.startedAt {
                Text(Date(milliseconds: started), style: .timer)
            }
        }
        .font(.footnote.monospacedDigit())
        .foregroundStyle(Palette.tertiary)
    }
}
