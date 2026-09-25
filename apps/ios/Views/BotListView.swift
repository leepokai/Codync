import CodyncKit
import CodyncUI
import SwiftUI

/// The roster: every bot on every computer is a person you can message (Grok Bot sidebar, phone-sized).
struct BotListView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @State private var editing: EditTarget?
    @State private var pickingComputer = false
    @State private var confirmDelete: RosterItem?

    /// Computers that can take a new bot right now.
    private var onlineStores: [BotStore] {
        accounts.computers.compactMap { accounts.store(for: $0.id) }.filter { $0.connection == .online }
    }

    var body: some View {
        let roster = accounts.roster
        ScrollView {
            LazyVStack(spacing: 0) {
                // One banner per computer that isn't reachable; the others keep working.
                ForEach(accounts.computers) { computer in
                    if let store = accounts.store(for: computer.id), store.connection != .online {
                        ConnectionBanner()
                            .environment(store)
                            .padding(.vertical, 4)
                    }
                }

                if roster.isEmpty {
                    EmptyRoster(canCreate: !onlineStores.isEmpty, hasComputer: !accounts.computers.isEmpty,
                                create: newBot, showComputers: { app.showComputers = true })
                }

                ForEach(roster) { item in
                    if let store = accounts.store(for: item.ref.computerId) {
                        row(item, store: store)
                    }
                }
            }
            .padding(.horizontal, 16)
        }
        .background(Palette.background)
        // Grok-style bare top bar: no visible title, just the buttons.
        .safeAreaInset(edge: .top, spacing: 0) {
            ScreenHeader {
                AccountSwitcherButton()
            } title: {
                EmptyView()
            } trailing: {
                IconButton("Computers", systemImage: "desktopcomputer") { app.showComputers = true }
                IconButton("New bot", systemImage: "plus", action: newBot)
                    .disabled(onlineStores.isEmpty)
            }
        }
        .hidesSystemNavigationBar()
        .refreshable {
            for computer in accounts.computers { accounts.store(for: computer.id)?.restartStream() }
            await accounts.refreshCloud()
        }
        .codyncSheet(item: $editing) { target in
            if let store = accounts.store(for: target.computerId) {
                BotEditorView(draft: target.draft)
                    .environment(store)
                    .onChange(of: store.selection) { _, botId in
                        // A new bot opens its chat, on the computer it was created on.
                        guard let botId else { return }
                        store.selection = nil
                        accounts.selection = BotReference(accountId: accounts.accountId, computerId: target.computerId, botId: botId)
                    }
            }
        }
        .codyncSheet(isPresented: $pickingComputer) {
            ComputerPicker(stores: onlineStores) { store in
                // Let the picker slide away before the editor slides up.
                Task {
                    try? await Task.sleep(for: .milliseconds(450))
                    editing = EditTarget(computerId: store.computer.id, draft: BotDraft())
                }
            }
        }
        .codyncDialog("Delete \(confirmDelete?.bot.name ?? "bot")?",
                      isPresented: Binding(get: { confirmDelete != nil }, set: { if !$0 { confirmDelete = nil } }),
                      message: "Files it changed on your computer stay as they are.") {
            let item = confirmDelete
            return [DialogAction("Delete bot and its conversation", destructive: true) {
                if let item { accounts.store(for: item.ref.computerId)?.delete(item.bot) }
            }]
        }
    }

    private func row(_ item: RosterItem, store: BotStore) -> some View {
        let bot = item.bot
        // A plain button instead of a NavigationLink: same push, no chevron.
        return Button { accounts.selection = item.ref } label: {
            VStack(alignment: .leading, spacing: 0) {
                BotRow(bot: bot)
                ComputerCaption(store: store)
                    .padding(.leading, 58)
                    .padding(.bottom, 6)
                    .offset(y: -6)
            }
            .environment(store)
        }
        .buttonStyle(.plain)
        .contextActions {
            [
                MenuItem(bot.pinned ? "Unpin" : "Pin", icon: bot.pinned ? "pin.slash" : "pin") { store.setPinned(bot, !bot.pinned) },
                MenuItem("Edit profile", icon: "pencil") { editing = EditTarget(computerId: item.ref.computerId, draft: BotDraft(bot)) },
                MenuItem("Mark as read", icon: "checkmark.message") { store.markRead(bot.id) },
                MenuItem("Hide from list", icon: "eye.slash") { store.setHidden(bot, true) },
                MenuItem("Delete", icon: "trash", destructive: true, divider: true) { confirmDelete = item },
            ]
        }
    }

    /// A new bot lives on one computer: pick it first when there's a choice.
    private func newBot() {
        let stores = onlineStores
        if stores.count == 1, let store = stores.first {
            editing = EditTarget(computerId: store.computer.id, draft: BotDraft())
        } else if stores.count > 1 {
            pickingComputer = true
        }
    }
}

private struct EditTarget: Identifiable {
    let id = UUID()
    let computerId: ComputerID
    let draft: BotDraft
}

/// "Which computer should it run on?" — only computers that are online.
private struct ComputerPicker: View {
    let stores: [BotStore]
    let pick: (BotStore) -> Void
    @Environment(\.dismissModal) private var dismiss

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Run it on")
            ScrollView {
                VStack(spacing: 4) {
                    ForEach(stores, id: \.computer.id) { store in
                        Button {
                            dismiss()
                            pick(store)
                        } label: {
                            HStack(spacing: 12) {
                                ComputerBadge(store.computer, size: 36)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(store.hostName).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                                    Text("\(store.roster.count) bot\(store.roster.count == 1 ? "" : "s")")
                                        .font(.subheadline).foregroundStyle(Palette.secondary)
                                }
                                Spacer()
                                RouteIcon(route: store.hostRoute).foregroundStyle(Palette.tertiary)
                            }
                            .padding(.horizontal, 20)
                            .padding(.vertical, 10)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(PressScale())
                    }
                }
            }
        }
    }
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
