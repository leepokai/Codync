import CodyncKit
import CodyncUI
import SwiftUI

/// The roster: every bot on every computer is a person you can message (Grok Bot sidebar, phone-sized).
/// A `List` with its styling taken away, for the system's reordering: touch and hold opens a bot's
/// menu, and dragging it (before or after the menu shows) moves it among its computer's bots with
/// the same pin state and section. Dragging a computer's heading moves the computer with its bots;
/// a section's heading folds its bots away and holds its menu.
struct BotListView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @State private var creating: NewTarget?
    @State private var editing: EditTarget?
    @State private var editingGroup: GroupTarget?
    @State private var confirmDelete: RosterItem?
    /// What the section name dialog names while it's open.
    @State private var naming: SectionNaming?
    /// The row that's lifted (a `RosterLine` id): a bot to drag or for its menu, or a computer's heading.
    @State private var lifted: String?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Computers left out of the list on this device, comma-separated IDs.
    @AppStorage("hiddenComputers") private var hiddenComputers = ""
    /// Computers whose bots are folded away on this device, comma-separated IDs.
    @AppStorage("collapsedComputers") private var collapsedComputers = ""
    /// Sections folded away on this device, by name, one per line: a name never holds a line break.
    // ponytail: one name folds that section on every computer; key it by computer if they must fold apart.
    @AppStorage("collapsedRosterSections") private var collapsedRosterSections = ""

    @AppStorage("rosterSectionOrder") private var sectionOrderData = Data()
    private var sectionOrder: RosterSectionOrder {
        get { RosterSectionOrder(data: sectionOrderData) }
        nonmutating set { sectionOrderData = newValue.data }
    }

    private var collapsedIDs: Set<ComputerID> { Set(collapsedComputers.split(separator: ",").map(String.init)) }

    private func toggleCollapsed(_ id: ComputerID) {
        var ids = collapsedIDs
        if ids.remove(id) == nil { ids.insert(id) }
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { collapsedComputers = ids.sorted().joined(separator: ",") }
    }

    private var foldedSections: Set<String> {
        get { Set(collapsedRosterSections.split(separator: "\n").map(String.init)) }
        nonmutating set { collapsedRosterSections = newValue.sorted().joined(separator: "\n") }
    }

    private func toggleSection(_ name: String) {
        var names = foldedSections
        if names.remove(name) == nil { names.insert(name) }
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { foldedSections = names }
    }

    /// Renames or removes (nil) a section on `store`'s computer; a folded one stays folded under its
    /// new name, on this device, at once. A refusal puts the folds back, unless one was folded or
    /// unfolded meanwhile.
    private func renameSection(_ name: String, to newName: String?, on store: BotStore) {
        Task {
            // Right before the call, which moves the bots before it waits: one update shows both.
            let orderBefore = sectionOrderData
            var renamedOrder = sectionOrder
            renamedOrder.set(sectionLines(store).compactMap { $0.sectionName(on: store.computer.id) }, on: store.computer.id)
            renamedOrder.rename(name, to: newName, on: store.computer.id)
            sectionOrder = renamedOrder
            let orderAfter = sectionOrderData
            let before = foldedSections
            let moved = RosterSection.folds(before, renaming: name, to: newName)
            foldedSections = moved
            if await store.renameSection(from: name, to: newName) { return }
            if sectionOrderData == orderAfter { sectionOrderData = orderBefore }
            foldedSections = RosterSection.folds(foldedSections, refused: moved, before: before)
        }
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

    /// The roster's rows: each shown computer's heading (with several computers), in the order
    /// they were dragged into, followed by its bots and sections unless it's folded away.
    private var lines: [RosterLine] {
        shownStores.flatMap { store -> [RosterLine] in
            let id = store.computer.id
            let rows = grouped && collapsedIDs.contains(id) ? [] : sectionLines(store)
            return grouped ? [.computer(id, store)] + rows : rows
        }
    }

    /// A computer's bots: pinned ones, then each section's heading with its bots (none while
    /// folded), then the bots in no section (under the "Bots" label when there are sections).
    private func sectionLines(_ store: BotStore) -> [RosterLine] {
        let id = store.computer.id
        let items = accounts.roster.filter { $0.ref.computerId == id }
        return RosterSection.lines(items, folded: foldedSections, order: sectionOrder.saved(on: id)) { $0.bot }.map { line -> RosterLine in
            switch line {
            case let .header(section): .section(section, id, store)
            case .label: .label(id)
            case let .bot(item): .bot(item, store)
            }
        }
    }

    var body: some View {
        let roster = accounts.roster.filter { shownIDs.contains($0.ref.computerId) }
        let lines = self.lines
        let movingComputer = lines.contains { $0.id == lifted && $0.computerId != nil }
        let movingSection = lines.first { $0.id == lifted }?.sectionComputer
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
                    case let .section(section, _, store):
                        if movingComputer { Color.clear.frame(height: 0) } else { sectionHeading(section, store: store) }
                    case .label:
                        if movingComputer { Color.clear.frame(height: 0) } else {
                            RosterBotsLabel().itemProvider { lift(line) }
                                .accessibilityActions {
                                    Button(L10n.tr("Move up")) { moveSection(line, by: -1) }
                                    Button(L10n.tr("Move down")) { moveSection(line, by: 1) }
                                }
                        }
                    case let .bot(item, store):
                        // A dragged heading folds every computer's bots into their headings. Each
                        // row stays, empty: taking it out would move the lifted heading's place in
                        // the list, and the system then drops the drag.
                        if movingComputer || movingSection == item.ref.computerId { Color.clear.frame(height: 0) } else { row(item, store: store) }
                    }
                }
                .onMove(perform: moveLines)
            }
            .listRowInsets(EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 16))
            .listRowSeparator(.hidden)
            .listRowBackground(Color.clear)
        }
        .reviewInvitation(blocked: creating != nil || editing != nil || editingGroup != nil
                          || confirmDelete != nil || lifted != nil)
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
        .sectionNameDialog($naming)
    }

    /// A row the system's reordering dropped. A heading moves its computer and its bots (an order
    /// kept on this device); a bot moves only among its computer's bots with the same pin state and
    /// section, an order the host syncs to every device: dropped among others (another section,
    /// another computer), it goes first or last of its own. Section headings and the Bots label move as groups on this device.
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
            store.drop(item.bot.id, in: after.compactMap { $0.botId(on: item.ref.computerId) })
        case .section, .label:
            guard let computer = before[from].sectionComputer else { return }
            let names = after.compactMap { $0.sectionName(on: computer) }
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { sectionOrder.set(names, on: computer) }
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

    /// A section's heading: tap folds its bots away (on this device); touch and hold to rename or
    /// remove it, its bots staying without one. An older computer has no section menu.
    @ViewBuilder private func sectionHeading(_ section: RosterSection, store: BotStore) -> some View {
        let header = RosterSectionHeader(section: section, hasMenu: store.supportsSections, toggle: { toggleSection(section.name) },
                                         moving: lines.first { $0.id == lifted }?.sectionComputer == store.computer.id)
            .itemProvider { lift(.section(section, store.computer.id, store)) }
            .accessibilityActions {
                Button(L10n.tr("Move up")) { moveSection(.section(section, store.computer.id, store), by: -1) }
                Button(L10n.tr("Move down")) { moveSection(.section(section, store.computer.id, store), by: 1) }
            }
        if store.supportsSections {
            header.contextActions {
                [
                    MenuItem("Rename section…", icon: "pencil") {
                        naming = SectionNaming(title: "Rename section", name: section.name) {
                            renameSection(section.name, to: $0, on: store)
                        }
                    },
                    MenuItem("Remove section", icon: "xmark.circle") { renameSection(section.name, to: nil, on: store) },
                ]
            }
        } else {
            header
        }
    }

    private func moveSection(_ line: RosterLine, by offset: Int) {
        guard let computer = line.sectionComputer, let name = line.sectionName(on: computer) else { return }
        var names = lines.compactMap { $0.sectionName(on: computer) }
        guard let from = names.firstIndex(of: name), names.indices.contains(from + offset) else { return }
        names.swapAt(from, from + offset)
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { sectionOrder.set(names, on: computer) }
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
        .contextActions {
            var items = [MenuItem(bot.pinned ? "Unpin" : "Pin", icon: bot.pinned ? "pin.slash" : "pin") { store.setPinned(bot, !bot.pinned) }]
            // Below Pin, like the desktop; an older computer has no sections.
            if store.supportsSections { items.append(sectionMenu(bot, store: store)) }
            items += [
                bot.isGroup
                    ? MenuItem("Edit group", icon: "person.2") { editingGroup = GroupTarget(computerId: item.ref.computerId, group: bot) }
                    : MenuItem("Edit profile", icon: "pencil") { editing = EditTarget(computerId: item.ref.computerId, draft: BotDraft(bot)) },
                MenuItem("Mark as read", icon: "checkmark.message") { store.markAllRead(bot.id) },
            ]
            if !bot.lead && !bot.isGroup && store.supportsLeadChoice {
                items.append(MenuItem("Make lead agent", icon: "star") { store.setLead(bot) })
            }
            // The lead stays in sight.
            if !bot.lead {
                items.append(MenuItem("Hide from list", icon: "eye.slash") { store.setHidden(bot, true) })
            }
            // An older computer keeps its lead.
            if !bot.lead || store.supportsLeadChoice {
                items.append(MenuItem("Delete", icon: "trash", destructive: true) { confirmDelete = item })
            }
            return items
        }
    }

    /// "Move to section ▸": the computer's other sections (by the names the user gave them, never
    /// translated), a new one, and out of its own.
    private func sectionMenu(_ bot: Bot, store: BotStore) -> MenuItem {
        var rows = store.sections.filter { $0 != bot.section }.map { name in
            MenuItem(name, icon: "tray", verbatim: true) { store.setSection(bot, to: name) }
        }
        rows.append(MenuItem("New section…", icon: "plus", divider: !rows.isEmpty) {
            naming = SectionNaming(title: "New section", name: "") { store.setSection(bot, to: $0) }
        })
        if bot.section != nil {
            rows.append(MenuItem("Remove from section", icon: "minus.circle") { store.setSection(bot, to: nil) })
        }
        return MenuItem("Move to section", icon: "tray", submenu: rows)
    }

    /// The system asks for a row's item as it lifts; the row shows as lifted until it lets go.
    /// A heading folds the bots away and back with an animation.
    private func lift(_ line: RosterLine) -> NSItemProvider {
        let id = line.id
        let animation = (line.computerId != nil || line.sectionComputer != nil) ? Motion.reduced(Motion.layout, reduceMotion) : nil
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

