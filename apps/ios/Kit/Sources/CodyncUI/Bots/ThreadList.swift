import CodyncKit
import SwiftUI

/// The Threads tab's list, the desktop's: what needs the user, then the groups the list's options make (folders and
/// routines, bots or computers; a group's heading folds it, on this device), then the rest, newest first, the last of
/// it on demand. A row is a thread's title (a routine's run: when it started) and one mark at its end for how it
/// stands. A row that needs the user opens its bot's chat at its card; one with a thread, the bot's details on that
/// thread; any other, the bot's details. The box of threads without a bot has no chat or details: its rows open their
/// thread over this list, and one without a thread opens nothing (the desktop's). Touch and hold: the thread's menu
/// (the desktop's `threadMenuItems`). While searching, `found` replaces the sections: one flat list of those rows.
public struct ThreadList: View {
    let sections: ThreadSections<RosterItem>
    let found: [ThreadSections<RosterItem>.Item]?
    let store: (RosterItem) -> BotStore?
    let openDetails: (RosterItem) -> Void
    /// The bot's chat, at the card `task` waits on when it needs the user.
    let openChat: (RosterItem, WorkTask) -> Void
    /// The bot's details, on the thread of `task`.
    let openThread: (RosterItem, WorkTask) -> Void
    /// Group headings folded on this device, by key, one per line.
    @AppStorage("foldedThreadSections") private var foldedKeys = ""
    @State private var showsAll = false
    @State private var question: WorkQuestion?
    /// A thread of the box without a bot, pushed over the list.
    @State private var looseThread: ThreadSections<RosterItem>.Item?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private static let shownRecent = 8

    public init(sections: ThreadSections<RosterItem>, found: [ThreadSections<RosterItem>.Item]? = nil, store: @escaping (RosterItem) -> BotStore?,
                openDetails: @escaping (RosterItem) -> Void, openChat: @escaping (RosterItem, WorkTask) -> Void,
                openThread: @escaping (RosterItem, WorkTask) -> Void) {
        self.sections = sections
        self.found = found
        self.store = store
        self.openDetails = openDetails
        self.openChat = openChat
        self.openThread = openThread
    }

    private var folded: Set<String> { Set(foldedKeys.split(separator: "\n").map(String.init)) }

    public var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if let found {
                    ForEach(found) { row($0) }
                } else {
                    sectionRows
                }
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 20)
        }
        .background(Palette.background)
        .workQuestion($question)
        .navigationDestination(isPresented: Binding(get: { looseThread != nil }, set: { if !$0 { looseThread = nil } })) {
            if let looseThread, let store = store(looseThread.of) {
                WorkThreadView(botId: looseThread.of.bot.id, task: looseThread.task, openInChat: nil)
                    .environment(store)
                    .toolbar(.hidden, for: .tabBar)
            }
        }
    }

    /// What needs the user, the groups, then the rest.
    @ViewBuilder private var sectionRows: some View {
        let recent = showsAll ? sections.recent : Array(sections.recent.prefix(Self.shownRecent))
        if !sections.needsYou.isEmpty {
            label("Needs you")
            ForEach(sections.needsYou) { row($0) }
        }
        ForEach(ThreadSections<RosterItem>.Group.Kind.allCases, id: \.self) { kind in
            let groups = sections.groups.filter { $0.kind == kind }
            if !groups.isEmpty {
                label(title(kind))
                ForEach(groups, id: \.key) { group($0) }
            }
        }
        if !sections.recent.isEmpty {
            label("Recent")
            ForEach(recent) { row($0) }
            if recent.count < sections.recent.count {
                Button { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { showsAll = true } } label: {
                    Text(L10n.tr("Show more")).foregroundStyle(Palette.tertiary).rowFrame()
                }
                .buttonStyle(.plain)
            }
        }
    }

    private func title(_ kind: ThreadSections<RosterItem>.Group.Kind) -> String {
        switch kind {
        case .folder: "Folders"
        case .routine: "Routines"
        case .bot: "Bots"
        case .computer: "Computers"
        }
    }

    private func label(_ title: String) -> some View {
        Text(L10n.tr(title))
            .font(.footnote.weight(.semibold))
            .foregroundStyle(Palette.tertiary)
            .padding(.top, 18)
            .padding(.bottom, 4)
            .accessibilityAddTraits(.isHeader)
    }

    @ViewBuilder private func group(_ group: ThreadSections<RosterItem>.Group) -> some View {
        let isFolded = folded.contains(group.key)
        let of = group.rows[0].of
        let plain = group.kind == .bot && of.bot.plain
        // Folded, its heading still says that something in it went wrong or works.
        let loud = isFolded ? (group.rows.first { $0.task.status == .failed } ?? group.rows.first { $0.task.status == .working }) : nil
        Button { toggle(group.key) } label: {
            HStack(spacing: 8) {
                switch group.kind {
                case .bot where !plain: BotAvatar(bot: of.bot, size: 18, animated: false)
                case .bot: Image(systemName: "person.crop.circle.dashed").foregroundStyle(Palette.secondary)
                case .folder: Image(systemName: "folder").foregroundStyle(Palette.secondary)
                case .routine: Image(systemName: "clock").foregroundStyle(Palette.secondary)
                case .computer: Image(systemName: "desktopcomputer").foregroundStyle(Palette.secondary)
                }
                Text(plain ? L10n.tr("No bot") : group.title)
                    .font(.body.weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                Spacer(minLength: 8)
                if let loud { WorkMark(status: loud.task.status) }
                Image(systemName: "chevron.down")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Palette.tertiary)
                    .rotationEffect(.degrees(isFolded ? -90 : 0))
            }
            .rowFrame()
        }
        .buttonStyle(.plain)
        .accessibilityValue(isFolded ? "Collapsed" : "Expanded")
        if !isFolded {
            ForEach(group.rows) { item in
                // A routine's runs are told apart by when they started.
                row(item, label: group.kind == .routine
                    ? RelativeTime.separator(Date(milliseconds: item.task.createdAt)) : item.task.title)
                    .padding(.leading, 26)
            }
        }
    }

    private func row(_ item: ThreadSections<RosterItem>.Item, label: String? = nil) -> some View {
        Button { open(item) } label: {
            HStack(spacing: 8) {
                Text(label ?? item.task.title).foregroundStyle(Palette.text).lineLimit(1)
                Spacer(minLength: 8)
                if item.task.status != .done { WorkMark(status: item.task.status) }
            }
            .rowFrame()
        }
        .buttonStyle(.plain)
        .contextActions { menu(item) }
    }

    private func open(_ item: ThreadSections<RosterItem>.Item) {
        if item.of.bot.plain {
            showThread(item)
        } else if item.task.status == .needsYou {
            openChat(item.of, item.task)
        } else if item.task.thread != nil {
            showThread(item)
        } else {
            openDetails(item.of)
        }
    }

    /// Its thread: over this list for the box without a bot, else in its bot's details.
    private func showThread(_ item: ThreadSections<RosterItem>.Item) {
        guard item.task.thread != nil else { return }
        if item.of.bot.plain { looseThread = item } else { openThread(item.of, item.task) }
    }

    /// Where it goes (its thread, its bot's chat), what its bot's box does with it (as its bot's details offer), what
    /// can be copied, and Drop last.
    private func menu(_ item: ThreadSections<RosterItem>.Item) -> [MenuItem] {
        let (task, bot) = (item.task, item.of.bot)
        guard let store = store(item.of) else { return [] }
        let actions = WorkSections(bot.work).actions(for: task, maxThreads: bot.maxThreads)
        let offline = store.isOffline
        let act = { (action: WorkAction) in store.run(action, on: task, of: bot) { question = $0 } }
        var go: [MenuItem] = []
        if task.thread != nil { go.append(MenuItem("Open thread", icon: "arrow.triangle.branch") { showThread(item) }) }
        // The box of threads without a bot has no chat of its own.
        if !bot.plain {
            go.append(MenuItem(task.status == .needsYou ? "Answer in chat" : "Open chat", icon: "bubble.left") { openChat(item.of, task) })
        }
        let inBox = actions.filter { $0 != .drop }.map { action in
            MenuItem(action.title, icon: action.symbol, destructive: action.destructive, enabled: !offline) { act(action) }
        }
        let copy = [MenuItem("Copy title", icon: "doc.on.doc") { Pasteboard.copy(task.title) }]
            + (task.wherePath.map { path in [MenuItem("Copy folder path", icon: "folder") { Pasteboard.copy(path) }] } ?? [])
        let drop = actions.contains(.drop) ? [MenuItem("Drop", icon: "xmark", destructive: true, enabled: !offline) { act(.drop) }] : []
        return [go, inBox, copy, drop].filter { !$0.isEmpty }.enumerated().flatMap { index, part in
            var part = part
            if index > 0 { part[0].divider = true }
            return part
        }
    }

    private func toggle(_ key: String) {
        var keys = folded
        if keys.remove(key) == nil { keys.insert(key) }
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { foldedKeys = keys.sorted().joined(separator: "\n") }
    }
}

private extension View {
    /// A row's height and tap area, full width.
    func rowFrame() -> some View {
        frame(maxWidth: .infinity, minHeight: 44, alignment: .leading).contentShape(Rectangle())
    }
}
