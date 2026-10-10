import CodyncKit
import CodyncUI
import SwiftUI

/// The roster: every bot on every computer is a person you can message (Grok Bot sidebar, phone-sized).
struct BotListView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @State private var editing: EditTarget?
    @State private var editingGroup: GroupTarget?
    @State private var confirmDelete: RosterItem?
    /// The computer section being dragged by its long-pressed heading.
    @State private var drag: SectionDrag?
    /// Each computer section's height, to work out where a dragged one lands.
    @State private var sectionHeights: [ComputerID: CGFloat] = [:]
    /// The bot row being dragged among its computer's bots (see `BotRowDrop`).
    @State private var draggingBot: BotReference?
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
        allStores.filter { $0.connection == .online && $0.mismatch == nil }
    }

    private var allStores: [BotStore] {
        accounts.computers.compactMap { accounts.store(for: $0.id) }
    }

    var body: some View {
        let roster = accounts.roster.filter { shownIDs.contains($0.ref.computerId) }
        ScrollView {
            LazyVStack(spacing: 0) {
                // A newer app release (dismissible; everything still works). Computers update
                // themselves, so a newer host gets no reminder here.
                AppUpdateReminder()
                if !grouped {
                    // One computer: its update notice leads the list.
                    ForEach(shownStores, id: \.computer.id) { updateCard($0) }
                }
                // A computer waiting for an update has its card instead; its bots aren't known yet.
                if roster.isEmpty && shownStores.allSatisfy({ $0.mismatch == nil }) {
                    EmptyRoster(canCreate: allStores.contains { $0.mismatch == nil }, hasComputer: !accounts.computers.isEmpty,
                                create: newBot, showComputers: { app.showComputers = true })
                }

                if grouped {
                    // A light heading per computer, in the order the user dragged them into.
                    let ids = shownStores.map(\.computer.id)
                    let landing = drag.map { landingIndex(ids, $0) }
                    ForEach(shownStores, id: \.computer.id) { store in
                        let id = store.computer.id
                        let collapsed = collapsedIDs.contains(id)
                        let lifted = drag?.id == id
                        VStack(spacing: 0) {
                            // Not synced while it needs an update: "No bots yet" wouldn't be true.
                            ComputerSection(store: store, empty: store.mismatch == nil && !roster.contains { $0.ref.computerId == id },
                                            collapsed: collapsed, toggle: { toggleCollapsed(id) }, move: move)
                                .modifier(ComputerSectionGesture(tap: { toggleCollapsed(id) }) { state, y in dragSection(id, state, y) })
                            updateCard(store)
                            if !collapsed {
                                ForEach(roster.filter { $0.ref.computerId == id }) { row($0, store: store) }
                            }
                        }
                        .background(lifted ? Palette.background : .clear, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                        .scaleEffect(lifted ? 0.94 : 1)
                        .shadow(color: .black.opacity(lifted ? 0.18 : 0), radius: 16, y: 6)
                        .animation(Motion.reduced(Motion.layout, reduceMotion), value: lifted)
                        .offset(y: sectionOffset(id, ids: ids, landing: landing))
                        .zIndex(lifted ? 1 : 0)
                        // The lifted section follows the finger; the others slide out of its way.
                        .transaction { if lifted { $0.animation = nil } }
                        .animation(Motion.reduced(Motion.layout, reduceMotion), value: landing)
                        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { sectionHeights[id] = $0 }
                    }
                } else {
                    ForEach(roster) { item in
                        if let store = accounts.store(for: item.ref.computerId) {
                            row(item, store: store)
                        }
                    }
                }
            }
            .padding(.horizontal, 16)
        }
        .scrollDisabled(drag != nil)
        .sensoryFeedback(.impact(weight: .medium), trigger: drag?.id) { _, new in new != nil }
        .sensoryFeedback(.selection, trigger: drag.map { landingIndex(shownStores.map(\.computer.id), $0) })
        .sensoryFeedback(.selection, trigger: roster.map(\.id)) { _, _ in draggingBot != nil }
        .background(Palette.background)
        .navigationTitle("Bots")
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
                Menu("New", systemImage: "plus") {
                    Button("New bot", systemImage: "plus", action: newBot)
                    Button("New group chat", systemImage: "person.2", action: newGroup)
                        .disabled(onlineStores.isEmpty)
                }
                // A computer that needs an update can't take a new bot.
                .disabled(!allStores.contains { $0.mismatch == nil })
            }
        }
        .refreshable {
            for computer in accounts.computers { accounts.store(for: computer.id)?.restartStream() }
            await accounts.refreshCloud()
        }
        .codyncSheet(item: $editing) { target in
            // A new bot can move to another computer until it's created.
            let computerId = editing?.computerId ?? target.computerId
            if let store = accounts.store(for: computerId) {
                BotEditorView(draft: target.draft,
                              computers: allStores.map {
                                  ($0.computer.id, $0.connection == .online ? $0.hostName : "\($0.hostName) (offline)")
                              },
                              computer: target.draft.id == nil
                                  ? Binding(get: { computerId }, set: { editing?.computerId = $0 })
                                  : nil)
                    .environment(store)
                    .onChange(of: store.selection) { _, botId in
                        // A new bot opens its chat, on the computer it was created on.
                        guard let botId else { return }
                        store.selection = nil
                        accounts.selection = BotReference(accountId: accounts.accountId, computerId: computerId, botId: botId)
                    }
            }
        }
        .codyncSheet(item: $editingGroup) { target in
            if let store = accounts.store(for: target.computerId) {
                GroupEditorView(group: target.group)
                    .environment(store)
                    .onChange(of: store.selection) { _, botId in
                        guard let botId else { return }
                        store.selection = nil
                        accounts.selection = BotReference(accountId: accounts.accountId, computerId: target.computerId, botId: botId)
                    }
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
    }

    /// Where the dragged section would land: past a neighbour once it's dragged over half of it.
    private func landingIndex(_ ids: [ComputerID], _ drag: SectionDrag) -> Int {
        guard let from = ids.firstIndex(of: drag.id) else { return 0 }
        var to = from, rest = drag.translation
        while to + 1 < ids.count, let h = sectionHeights[ids[to + 1]], rest > h / 2 { rest -= h; to += 1 }
        while to > 0, let h = sectionHeights[ids[to - 1]], rest < -h / 2 { rest += h; to -= 1 }
        return to
    }

    private func sectionOffset(_ id: ComputerID, ids: [ComputerID], landing: Int?) -> CGFloat {
        guard let drag, let landing, let from = ids.firstIndex(of: drag.id), let i = ids.firstIndex(of: id) else { return 0 }
        if id == drag.id { return drag.translation }
        let h = sectionHeights[drag.id] ?? 0
        if from < i && i <= landing { return -h }
        if landing <= i && i < from { return h }
        return 0
    }

    private func dragSection(_ id: ComputerID, _ state: UIGestureRecognizer.State, _ y: CGFloat) {
        switch state {
        case .began:
            drag = SectionDrag(id: id, startY: y, y: y)
        case .changed:
            drag?.y = y
        case .ended:
            guard var drag, drag.id == id else { return }
            drag.y = y
            let ids = shownStores.map(\.computer.id)
            let to = landingIndex(ids, drag)
            // The order changes as the offsets drop, so everything settles where it's shown.
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                if ids.contains(id), ids.indices.contains(to), ids[to] != id {
                    accounts.move(id, to: ids[to])
                }
                self.drag = nil
            }
        case .cancelled, .failed:
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { drag = nil }
        default:
            break
        }
    }

    @ViewBuilder private func updateCard(_ store: BotStore) -> some View {
        if let mismatch = store.mismatch {
            UpdateNeededCard(store: store, mismatch: mismatch)
                .padding(.vertical, 8)
                .transition(.opacity)
        }
    }

    private func row(_ item: RosterItem, store: BotStore) -> some View {
        let bot = item.bot
        // A plain button instead of a NavigationLink: same push, no chevron.
        return Button { accounts.selection = item.ref } label: {
            BotRow(bot: bot)
                .environment(store)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accounts.computers.count > 1 ? "\(bot.name), on \(store.hostName)" : bot.name)
        .accessibilityActions {
            Button("Move up") { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { store.move(bot.id, by: -1) } }
            Button("Move down") { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { store.move(bot.id, by: 1) } }
        }
        .contextActions {
            [
                MenuItem(bot.pinned ? "Unpin" : "Pin", icon: bot.pinned ? "pin.slash" : "pin") { store.setPinned(bot, !bot.pinned) },
                bot.isGroup
                    ? MenuItem("Edit group", icon: "person.2") { editingGroup = GroupTarget(computerId: item.ref.computerId, group: bot) }
                    : MenuItem("Edit profile", icon: "pencil") { editing = EditTarget(computerId: item.ref.computerId, draft: BotDraft(bot)) },
                MenuItem("Mark as read", icon: "checkmark.message") { store.markAllRead(bot.id) },
                MenuItem("Hide from list", icon: "eye.slash") { store.setHidden(bot, true) },
                MenuItem("Delete", icon: "trash", destructive: true, divider: true) { confirmDelete = item },
            ]
        }
        // Touch and hold, then drag: the menu gives way to the drag (iOS's usual pairing).
        .onDrag {
            draggingBot = item.ref
            return NSItemProvider(object: bot.name as NSString)
        }
        .onDrop(of: [.text], delegate: BotRowDrop(target: item, store: store, dragging: $draggingBot, reduceMotion: reduceMotion))
    }

    /// A group chat gathers bots of one computer (the first one online; more computers are future work).
    private func newGroup() {
        guard let store = onlineStores.first else { return }
        Task {
            // Let the menu fold away before the editor slides up.
            try? await Task.sleep(for: .milliseconds(200))
            editingGroup = GroupTarget(computerId: store.computer.id, group: nil)
        }
    }

    /// A new bot starts on the first computer online; the editor's Computer row can move it.
    private func newBot() {
        guard let store = onlineStores.first ?? allStores.first(where: { $0.mismatch == nil }) else { return }
        editing = EditTarget(computerId: store.computer.id, draft: BotDraft())
    }
}

/// A computer's heading above its bots: badge, name and connection. Tap folds its bots away;
/// long-press lifts the whole section to drag it up or down.
private struct ComputerSection: View {
    let store: BotStore
    let empty: Bool
    let collapsed: Bool
    let toggle: () -> Void
    let move: (ComputerID, Int) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            // The parent's tap/hold gesture owns touch input, so a release
            // after dragging cannot also trigger a nested button's collapse action.
            heading
            if empty && !collapsed {
                Text("No bots yet")
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
            Button("Move up") { move(store.computer.id, -1) }
            Button("Move down") { move(store.computer.id, 1) }
        }
    }

    private var heading: some View {
        HStack(spacing: 6) {
            ComputerBadge(store.computer, size: 18)
            Text(store.hostName)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
            Text(store.statusText)
                .font(.footnote)
                .foregroundStyle(store.isOffline ? Palette.warning : Palette.tertiary)
                .lineLimit(1)
                .contentTransition(.opacity)
            if store.connection == .online {
                RouteIcon(route: store.hostRoute).font(.caption2).foregroundStyle(Palette.tertiary)
            }
            Spacer(minLength: 8)
            Image(systemName: "chevron.down")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.tertiary)
                .rotationEffect(.degrees(collapsed ? -90 : 0))
                .accessibilityHidden(true)
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .contentShape(Rectangle())
    }
}

private struct SectionDrag {
    let id: ComputerID
    let startY: CGFloat
    var y: CGFloat
    var translation: CGFloat { y - startY }
}

private struct GroupTarget: Identifiable {
    let id = UUID()
    let computerId: ComputerID
    let group: Bot?
}

private struct EditTarget: Identifiable {
    let id = UUID()
    var computerId: ComputerID
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
            Text("No bots yet").font(.title3.weight(.semibold)).foregroundStyle(Palette.text)
            Text(hasComputer
                 ? "Create a bot for each kind of work — a reviewer, a fixer, a docs writer — and point it at a project."
                 : "Ask one of your account's computers for access, or pair one with its code.")
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
            if canCreate {
                Button("Create your first bot", action: create)
                    .buttonStyle(.primary)
            } else if !hasComputer {
                Button("Computers", action: showComputers)
                    .buttonStyle(.primary)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
    }
}
