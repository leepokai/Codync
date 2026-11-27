import CodyncKit
import SwiftUI

/// The Threads tab's list, the desktop's: what needs the user, then the groups the list's options make (folders and
/// routines, bots or computers; a group's heading folds it, on this device), then the rest, newest first, the last of
/// it on demand. A row is a thread's title (a routine's run: when it started) and one mark at its end for how it
/// stands. A row with a thread pushes its thread chat (`ThreadChatView`) over this list; one without, its bot's chat at
/// its card when it needs the user, else the bot's details. The box of threads without a bot has no chat or details:
/// its row without a thread opens nothing (the desktop's). Touch and hold: the thread's menu
/// (the desktop's `threadMenuItems`). While searching, `found` replaces the sections: one flat list of those rows.
public struct ThreadList: View {
    let sections: ThreadSections<RosterItem>
    let found: [ThreadSections<RosterItem>.Item]?
    let store: (RosterItem) -> BotStore?
    let openDetails: (RosterItem) -> Void
    /// The bot's chat, at the card `task` waits on when it needs the user.
    let openChat: (RosterItem, WorkTask) -> Void
    /// The list reached its end: older threads load from there.
    let loadMore: (() -> Void)?
    /// Group headings folded on this device, by key, one per line.
    @AppStorage("foldedThreadSections") private var foldedKeys = ""
    @State private var showsAll = false
    @State private var question: WorkQuestion?
    /// The thread pushed over the list (one just started there too).
    @Binding var thread: ThreadTarget?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private static let shownRecent = 8

    public init(sections: ThreadSections<RosterItem>, found: [ThreadSections<RosterItem>.Item]? = nil, store: @escaping (RosterItem) -> BotStore?,
                openDetails: @escaping (RosterItem) -> Void, openChat: @escaping (RosterItem, WorkTask) -> Void,
                thread: Binding<ThreadTarget?>, loadMore: (() -> Void)? = nil) {
        self.loadMore = loadMore
        self.sections = sections
        _thread = thread
        self.found = found
        self.store = store
        self.openDetails = openDetails
        self.openChat = openChat
    }

    private var folded: Set<String> { Set(foldedKeys.split(separator: "\n").map(String.init)) }

    public var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                if let found {
                    rows(found, in: "found")
                } else {
                    sectionRows
                }
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 20)
        }
        .background(Palette.background)
        // The Bots tab's top edge: the list fades softly under the bar.
        .scrollEdgeEffectStyle(.soft, for: .top)
        .workQuestion($question)
    }

    /// What needs the user, the groups, then the rest.
    @ViewBuilder private var sectionRows: some View {
        let recent = showsAll ? sections.recent : Array(sections.recent.prefix(Self.shownRecent))
        if !sections.needsYou.isEmpty {
            label("Needs you")
            rows(sections.needsYou, in: "needs")
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
            rows(recent, in: "recent")
            if recent.count < sections.recent.count {
                Button { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { showsAll = true } } label: {
                    Text(L10n.tr("Show more")).foregroundStyle(Palette.tertiary).rowFrame()
                }
                .buttonStyle(.plain)
            }
        }
        // Not while Recent hides rows behind Show more. A new id each time rows come, so an end still in view asks
        // again. ponytail: rows landing in folded groups keep the end in view and page on; fine at 30 a page.
        if let loadMore, recent.count == sections.recent.count {
            Color.clear.frame(height: 1).onAppear(perform: loadMore).id(rowCount)
        }
    }

    private var rowCount: Int {
        sections.needsYou.count + sections.groups.reduce(0) { $0 + $1.rows.count } + sections.recent.count
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
                case .bot: NoBotAvatar(size: 18)
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
            // A routine's runs are told apart by when they started.
            rows(group.rows, in: "group/\(group.key)", indent: 26) { item in
                group.kind == .routine ? RelativeTime.separator(Date(milliseconds: item.task.createdAt)) : item.task.title
            }
        }
    }

    /// One part's rows (`part`: Needs you, a group, Recent, the search's), with ids of that part's own: the same thread
    /// in two parts, or moving between them as it changes, is never a row the lazy stack reuses with the other part's
    /// layout (a group's row drawn without its indent).
    private func rows(_ items: [ThreadSections<RosterItem>.Item], in part: String, indent: CGFloat = 0,
                      label: ((ThreadSections<RosterItem>.Item) -> String)? = nil) -> some View {
        ForEach(items.map { (id: "\(part)/\($0.id)", item: $0) }, id: \.id) { entry in
            row(entry.item, label: label?(entry.item), indent: indent)
        }
    }

    private func row(_ item: ThreadSections<RosterItem>.Item, label: String?, indent: CGFloat) -> some View {
        Button { open(item) } label: {
            HStack(spacing: 8) {
                // The coding agent it runs on; its place kept without one, so titles line up.
                Group {
                    if let agent = item.task.agent(of: item.of.bot) {
                        AgentIcon(registry: store(item.of)?.agentRegistry(agent) ?? agent, size: 16)
                    } else {
                        Color.clear
                    }
                }
                .frame(width: 16, height: 16)
                Text(label ?? item.task.title).foregroundStyle(Palette.text).lineLimit(1)
                Spacer(minLength: 8)
                if item.task.status != .done { WorkMark(status: item.task.status) }
            }
            .padding(.leading, indent)
            .rowFrame()
        }
        .buttonStyle(.plain)
        .contextActions { menu(item) }
    }

    private func open(_ item: ThreadSections<RosterItem>.Item) {
        if item.task.thread != nil {
            thread = ThreadTarget(of: item.of, task: item.task)
        } else if item.of.bot.plain {
            return
        } else if item.task.status == .needsYou {
            openChat(item.of, item.task)
        } else {
            openDetails(item.of)
        }
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
        if task.thread != nil { go.append(MenuItem("Open thread", icon: "arrow.triangle.branch") { thread = ThreadTarget(of: item.of, task: task) }) }
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
