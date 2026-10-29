import CodyncKit
import CodyncUI
import SwiftUI

/// The roster: every bot on every computer is a person you can message (Grok Bot sidebar, phone-sized).
/// A `List` with its styling taken away, for the system's reordering: touch and hold opens a bot's
/// menu, and dragging it (before or after the menu shows) moves it among its computer's bots.
/// Dragging a computer's heading moves the computer with its bots.
struct BotListView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @State private var creating: NewTarget?
    @State private var editing: EditTarget?
    @State private var editingGroup: GroupTarget?
    @State private var confirmDelete: RosterItem?
    /// A bot whose agent starts over with a fresh context, once confirmed.
    @State private var confirmNewSession: RosterItem?
    /// The row that's lifted (a `RosterLine` id): a bot to drag or for its menu, or a computer's heading.
    @State private var lifted: String?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Computers left out of the list on this device, comma-separated IDs.
    @AppStorage("hiddenComputers") private var hiddenComputers = ""
    /// Computers whose bots are folded away on this device, comma-separated IDs.
    @AppStorage("collapsedComputers") private var collapsedComputers = ""

    private var collapsedIDs: Set<ComputerID> { Set(collapsedComputers.split(separator: ",").map(String.init)) }

    private func toggleCollapsed(_ id: ComputerID) {
        var ids = collapsedIDs
        if ids.remove(id) == nil { ids.insert(id) }
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { collapsedComputers = ids.sorted().joined(separator: ",") }
    }

    private var shownIDs: Set<ComputerID> {
        Set(ComputerSelection(all: accounts.computers.map(\.id), hidden: hiddenComputers).shown)
    }

    private var shownStores: [BotStore] {
        accounts.computers.filter { shownIDs.contains($0.id) }.compactMap { accounts.store(for: $0.id) }
    }

    /// One computer shows a flat list; more get a heading each.
    private var grouped: Bool { shownStores.count > 1 }

    /// Moves a computer one place up (-1) or down (+1) among the shown ones.
    private func move(_ id: ComputerID, _ offset: Int) {
        let ids = shownStores.map(\.computer.id)
        guard let i = ids.firstIndex(of: id), ids.indices.contains(i + offset) else { return }
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { accounts.move(id, to: ids[i + offset]) }
    }

    /// Computers that can take a new bot right now.
    private var onlineStores: [BotStore] {
        availableStores.filter { $0.connection == .online }
    }

    /// Computers that can take a new bot once they're connected (not ones waiting for an update, or
    /// ones that won't come back: they turned this iPhone away, or are this computer's older identity).
    private var availableStores: [BotStore] {
        accounts.computers.compactMap { accounts.store(for: $0.id) }.filter { $0.mismatch == nil && !accounts.isStale($0.computer.id) }
    }

    /// The rows the user can drag: each shown computer's heading (with several computers), in the
    /// order they were dragged into, followed by its bots unless it's folded away.
    private var lines: [RosterLine] {
        shownStores.flatMap { store -> [RosterLine] in
            let id = store.computer.id
            let bots = grouped && collapsedIDs.contains(id)
                ? [] : accounts.roster.filter { $0.ref.computerId == id }.map { RosterLine.bot($0, store) }
            return grouped ? [.computer(id, store)] + bots : bots
        }
    }

    var body: some View {
        let roster = accounts.roster.filter { shownIDs.contains($0.ref.computerId) }
        let lines = self.lines
        let movingComputer = lines.contains { $0.id == lifted && $0.computerId != nil }
        List {
            Group {
                // A newer app release (dismissible; everything still works). Computers update
                // themselves, so a newer host gets no reminder here.
                AppUpdateReminder()
                if !grouped {
                    // One computer: its update notice leads the list.
                    ForEach(shownStores, id: \.computer.id) { updateCard($0) }
                }
                // A computer waiting for an update has its card instead; its bots aren't known yet.
                if roster.isEmpty && shownStores.allSatisfy({ $0.mismatch == nil }) {
                    // Computers that turned this iPhone away count as none: the way on is in Computers.
                    EmptyRoster(canCreate: !availableStores.isEmpty, hasComputer: accounts.computers.contains { !accounts.isStale($0.id) },
                                create: newBot, showComputers: { app.showComputers = true })
                }
                ForEach(lines) { line in
                    switch line {
                    case let .computer(_, store): computerHeading(store, roster: roster, folded: movingComputer)
                    case let .bot(item, store):
                        // A dragged heading folds every computer's bots into their headings. Each
                        // row stays, empty: taking it out would move the lifted heading's place in
                        // the list, and the system then drops the drag.
                        if movingComputer { Color.clear.frame(height: 0) } else { row(item, store: store) }
                    }
                }
                .onMove(perform: moveLines)
            }
            .listRowInsets(EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16))
            .listRowSeparator(.hidden)
            .listRowBackground(Color.clear)
        }
        .reviewInvitation(blocked: creating != nil || editing != nil || editingGroup != nil
                          || confirmDelete != nil || confirmNewSession != nil || lifted != nil)
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .environment(\.defaultMinListRowHeight, 0)
        .background(Palette.background)
        .navigationTitle(L10n.tr("Bots"))
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.visible, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                AccountSwitcherButton()
            }
            ToolbarItem(placement: .principal) {
                ComputerFilterHeader(accounts: accounts, hidden: $hiddenComputers) { app.showComputers = true }
            }
            ToolbarItem(placement: .topBarTrailing) {
                // A native menu: the bar hosts toolbar buttons outside SwiftUI's layout,
                // so an anchored Codync menu can't find where the button is.
                Menu(L10n.tr("New"), systemImage: "plus") {
                    Button(L10n.tr("New bot"), systemImage: "plus", action: newBot)
                    Button(L10n.tr("New group chat"), systemImage: "person.2", action: newGroup)
                        .disabled(onlineStores.isEmpty)
                }
                // A computer that needs an update can't take a new bot.
                .disabled(availableStores.isEmpty)
            }
        }
        .refreshable {
            for computer in accounts.computers { accounts.store(for: computer.id)?.restartStream() }
            await accounts.refreshCloud()
        }
        .codyncSheet(item: $creating) { target in
            if let store = accounts.store(for: target.computerId) {
                Group {
                    switch target.kind {
                    case .bot:
                        // A new bot can move to another computer until it's created: a new target
                        // (same sheet) keeps the sheet on that computer while it slides away.
                        NewBotView(computers: availableStores.map {
                                       ($0.computer.id, $0.connection == .online ? $0.hostName : "\($0.hostName) (offline)")
                                   },
                                   computer: Binding(get: { target.computerId },
                                                     set: { creating = NewTarget(kind: .bot, computerId: $0) }))
                    case .group:
                        NewGroupView()
                    }
                }
                .environment(store)
            }
        }
        // A new bot or group selects itself; open its chat on the computer it was created on. Watched
        // here, not in the sheet: the dialog closes as it selects, and a closing sheet sees no change.
        .onChange(of: availableStores.map(\.selection)) {
            for store in availableStores {
                guard let botId = store.selection else { continue }
                store.selection = nil
                accounts.selection = BotReference(accountId: accounts.accountId, computerId: store.computer.id, botId: botId)
            }
        }
        .codyncSheet(item: $editing) { target in
            if let store = accounts.store(for: target.computerId) {
                BotEditorView(draft: target.draft).environment(store)
            }
        }
        .codyncSheet(item: $editingGroup) { target in
            if let store = accounts.store(for: target.computerId) {
                GroupEditorView(group: target.group).environment(store)
            }
        }
        .codyncDialog("Delete \(confirmDelete?.bot.name ?? "bot")?",
                      isPresented: Binding(get: { confirmDelete != nil }, set: { if !$0 { confirmDelete = nil } }),
                      message: confirmDelete?.bot.isGroup == true ? "Its bots and their own chats stay." : "Files it changed on your computer stay as they are.") {
            let item = confirmDelete
            return [DialogAction(item?.bot.isGroup == true ? "Delete group chat" : "Delete bot and its conversation", destructive: true) {
                if let item { accounts.store(for: item.ref.computerId)?.delete(item.bot) }
            }]
        }
        .codyncDialog("Start a new session?",
                      isPresented: Binding(get: { confirmNewSession != nil }, set: { if !$0 { confirmNewSession = nil } }),
                      message: "The conversation stays here, but the agent starts with a fresh context.") {
            let item = confirmNewSession
            return [DialogAction("New session") {
                if let item { accounts.store(for: item.ref.computerId)?.newSession(item.bot.id) }
            }]
        }
    }

    /// A row the system's reordering dropped. A heading moves its computer and its bots (an order
    /// kept on this device); a bot moves only among its computer's bots, an order the host syncs to
    /// every device: dropped above or below them (on another computer), it goes first or last.
    private func moveLines(from source: IndexSet, to destination: Int) {
        let before = lines
        guard let from = source.first, before.indices.contains(from) else { return }
        var after = before
        after.move(fromOffsets: source, toOffset: destination)
        switch before[from] {
        case let .computer(id, _):
            let old = before.compactMap(\.computerId), new = after.compactMap(\.computerId)
            guard let to = new.firstIndex(of: id), old[to] != id else { return }
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { accounts.move(id, to: old[to]) }
        case let .bot(item, store):
            store.reorder(after.compactMap { $0.botId(on: item.ref.computerId) })
        }
    }

    @ViewBuilder private func updateCard(_ store: BotStore) -> some View {
        if let mismatch = store.mismatch {
            UpdateNeededCard(store: store, mismatch: mismatch)
                .padding(.vertical, 8)
                .transition(.opacity)
        }
    }

    /// A light heading per computer, with its update notice. Dragging it moves the computer with its
    /// bots: while it's lifted (`folded`), every computer's bots fold into their headings.
    private func computerHeading(_ store: BotStore, roster: [RosterItem], folded: Bool) -> some View {
        let id = store.computer.id
        return VStack(spacing: 0) {
            // Not synced while it needs an update: "No bots yet" wouldn't be true.
            ComputerSection(store: store, empty: store.mismatch == nil && !roster.contains { $0.ref.computerId == id },
                            collapsed: folded || collapsedIDs.contains(id), toggle: { toggleCollapsed(id) }, move: move,
                            extensions: store.connection == .online && store.mismatch == nil ? { app.marketplace = id } : nil)
                .contentShape(Rectangle())
                .onTapGesture { toggleCollapsed(id) }
            updateCard(store)
        }
        .itemProvider { lift(.computer(id, store)) }
    }

    private func row(_ item: RosterItem, store: BotStore) -> some View {
        let bot = item.bot
        let line = RosterLine.bot(item, store)
        let card = RoundedRectangle(cornerRadius: 16, style: .continuous)
        // A plain button instead of a NavigationLink: same push, no chevron.
        return Button { accounts.selection = item.ref } label: {
            BotRow(bot: bot)
                .environment(store)
        }
        .buttonStyle(.plain)
        // Lifted, it's a rounded card with room around the avatar.
        .padding(.horizontal, 8)
        .background(lifted == line.id ? Palette.surface : .clear, in: card)
        .contentShape([.dragPreview, .contextMenuPreview], card)
        .padding(.horizontal, -8)
        .itemProvider { lift(line) }
        .accessibilityLabel(accounts.computers.count > 1 ? "\(bot.name), on \(store.hostName)" : bot.name)
        .accessibilityActions {
            Button(L10n.tr("Move up")) { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { store.move(bot.id, by: -1) } }
            Button(L10n.tr("Move down")) { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { store.move(bot.id, by: 1) } }
        }
        // The lead stays on top.
        .moveDisabled(bot.lead)
        .contextActions {
            // The lead is always on top and always there: no pin, hide or delete.
            var items = bot.lead ? [] : [MenuItem(bot.pinned ? "Unpin" : "Pin", icon: bot.pinned ? "pin.slash" : "pin") { store.setPinned(bot, !bot.pinned) }]
            if !bot.isGroup {
                items.append(MenuItem("New session", icon: "arrow.counterclockwise") { confirmNewSession = item })
            }
            items += [
                bot.isGroup
                    ? MenuItem("Edit group", icon: "person.2") { editingGroup = GroupTarget(computerId: item.ref.computerId, group: bot) }
                    : MenuItem("Edit profile", icon: "pencil") { editing = EditTarget(computerId: item.ref.computerId, draft: BotDraft(bot)) },
                MenuItem("Mark as read", icon: "checkmark.message") { store.markAllRead(bot.id) },
            ]
            if !bot.lead {
                items += [
                    MenuItem("Hide from list", icon: "eye.slash") { store.setHidden(bot, true) },
                    MenuItem("Delete", icon: "trash", destructive: true, divider: true) { confirmDelete = item },
                ]
            }
            return items
        }
    }

    /// The system asks for a row's item as it lifts; the row shows as lifted until it lets go.
    /// A heading folds the bots away and back with an animation.
    private func lift(_ line: RosterLine) -> NSItemProvider {
        let id = line.id
        let animation = line.computerId != nil ? Motion.reduced(Motion.layout, reduceMotion) : nil
        withAnimation(animation) { lifted = id }
        return LiftItem { Task { @MainActor in if lifted == id { withAnimation(animation) { lifted = nil } } } }
    }

    /// A group chat gathers bots of one computer (the first one online; more computers are future work).
    private func newGroup() {
        guard let store = onlineStores.first else { return }
        Task {
            // Let the menu fold away before the dialog slides up.
            try? await Task.sleep(for: .milliseconds(200))
            creating = NewTarget(kind: .group, computerId: store.computer.id)
        }
    }

    /// A new bot starts on the first computer online; the dialog's Computer row can move it.
    private func newBot() {
        guard let store = onlineStores.first ?? availableStores.first else { return }
        creating = NewTarget(kind: .bot, computerId: store.computer.id)
    }
}

/// The item a lifted row carries (nothing): the system lets go of it when the drag or the
/// menu ends.
private final class LiftItem: NSItemProvider {
    private let ended: @Sendable () -> Void

    init(ended: @escaping @Sendable () -> Void) {
        self.ended = ended
        super.init()
    }

    deinit { ended() }
}

/// One row the user can drag in the roster: a computer's heading or a bot.
private enum RosterLine: Identifiable {
    case computer(ComputerID, BotStore)
    case bot(RosterItem, BotStore)

    var id: String {
        switch self {
        case let .computer(id, _): "computer:\(id)"
        case let .bot(item, _): "bot:\(item.ref.computerId):\(item.bot.id)"
        }
    }

    /// The computer a heading stands for.
    var computerId: ComputerID? {
        if case let .computer(id, _) = self { id } else { nil }
    }

    func botId(on computerId: ComputerID) -> String? {
        if case let .bot(item, _) = self, item.ref.computerId == computerId { item.bot.id } else { nil }
    }
}

/// A computer's heading above its bots: badge, name and connection. Tap folds its bots away;
/// touch and hold, then drag, moves the computer (on this device).
private struct ComputerSection: View {
    let store: BotStore
    let empty: Bool
    let collapsed: Bool
    let toggle: () -> Void
    let move: (ComputerID, Int) -> Void
    /// This computer's own Extensions; nil while it can't be reached.
    let extensions: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            heading
            if empty && !collapsed {
                Text(L10n.tr("No bots yet"))
                    .font(.footnote)
                    .foregroundStyle(Palette.tertiary)
                    .padding(.horizontal, 8)
                    .padding(.bottom, 4)
                    .transition(.opacity)
            }
        }
        .padding(.top, 14)
        .padding(.bottom, 2)
        .padding(.horizontal, -8)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits([.isHeader, .isButton])
        .accessibilityLabel("\(store.hostName), \(store.statusText)")
        .accessibilityValue(collapsed ? "Collapsed" : "Expanded")
        .accessibilityHint("Double-tap to fold or expand. Touch and hold, then drag to reorder on this device.")
        .accessibilityAction(named: collapsed ? "Expand" : "Collapse", toggle)
        .accessibilityAction(.default) { toggle() }
        .accessibilityActions {
            Button(L10n.tr("Move up")) { move(store.computer.id, -1) }
            Button(L10n.tr("Move down")) { move(store.computer.id, 1) }
            if let extensions { Button(L10n.tr("Extensions"), action: extensions) }
        }
    }

    private var heading: some View {
        HStack(spacing: 6) {
            ComputerBadge(store.computer, size: 18)
            Text(store.hostName)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
            // The connection sits at the trailing edge, before the heading's buttons.
            Spacer(minLength: 8)
            Text(L10n.tr(store.statusText))
                .font(.footnote)
                .foregroundStyle(store.isOffline && !store.isUpdating ? Palette.warning : Palette.tertiary)
                .lineLimit(1)
                .contentTransition(.opacity)
            if store.connection == .online {
                RouteIcon(route: store.hostRoute, cloud: store.computer.cloud).font(.caption2).foregroundStyle(Palette.tertiary)
            }
            Image(systemName: "chevron.down")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.tertiary)
                .rotationEffect(.degrees(collapsed ? -90 : 0))
                .accessibilityHidden(true)
            if let extensions {
                // After the chevron, as on the desktop; sized like it so the heading stays one light line.
                Button(action: extensions) {
                    Image(systemName: "square.grid.2x2")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Palette.secondary)
                        .frame(width: 28, height: 22)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.borderless)
                .accessibilityLabel(L10n.tr("Extensions"))
                .help(L10n.tr("Extensions"))
            }
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .contentShape(Rectangle())
    }
}

private struct NewTarget: Identifiable {
    enum Kind { case bot, group }
    let id = UUID()
    let kind: Kind
    let computerId: ComputerID
}

private struct GroupTarget: Identifiable {
    let id = UUID()
    let computerId: ComputerID
    let group: Bot
}

private struct EditTarget: Identifiable {
    let id = UUID()
    let computerId: ComputerID
    let draft: BotDraft
}

private struct EmptyRoster: View {
    let canCreate: Bool
    let hasComputer: Bool
    let create: () -> Void
    let showComputers: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            CharacterAvatar(shape: "cloud", color: "green", size: 72)
            Text(L10n.tr("No bots yet")).font(.title3.weight(.semibold)).foregroundStyle(Palette.text)
            Text(L10n.tr(hasComputer
                 ? "Create a bot for each kind of work — a reviewer, a fixer, a docs writer — and point it at a project."
                 : "Ask one of your account's computers for access, or pair one with its code."))
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
            if canCreate {
                Button(L10n.tr("Create your first bot"), action: create)
                    .buttonStyle(.primary)
            } else if !hasComputer {
                Button(L10n.tr("Computers"), action: showComputers)
                    .buttonStyle(.primary)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
    }
}
