import CodyncKit
import CodyncUI
import SwiftUI
import WidgetKit

struct RootView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @AppStorage("onboardingCompleted") private var onboardingCompleted = false

    /// The open bot, as a NavigationStack path.
    private var path: Binding<[BotReference]> {
        Binding(get: { accounts.selection.map { [$0] } ?? [] }, set: { accounts.selection = $0.last })
    }

    var body: some View {
        Group {
            if accounts.computers.isEmpty && accounts.cloudComputers.isEmpty {
                NavigationStack {
                    PairingView(introductory: !onboardingCompleted)
                        .toolbar {
                            if onboardingCompleted {
                                ToolbarItem(placement: .topBarLeading) { AccountSwitcherButton() }
                            }
                        }
                }
            } else {
                TabView(selection: Bindable(app).tab) {
                    Tab("Bots", systemImage: "bubble.left.and.bubble.right.fill", value: .bots) {
                        NavigationStack(path: path) {
                            BotListView()
                                .navigationDestination(for: BotReference.self) { ref in
                                    ChatScreen(ref: ref)
                                        .toolbar(.hidden, for: .tabBar)
                                }
                        }
                    }
                    Tab("Usage", systemImage: "chart.bar.fill", value: .usage) {
                        NavigationStack { UsageTab() }
                    }
                }
                // Opening a bot (notification, widget, link) always lands on the Bots tab.
                .onChange(of: accounts.selection) { _, ref in if ref != nil { app.tab = .bots } }
            }
        }
        .background(Palette.background)
        .onChange(of: accounts.computers.isEmpty, initial: true) { _, empty in
            // Existing installations have already completed setup. Keep this
            // device-level milestone across account changes and unpairing.
            if !empty { onboardingCompleted = true }
        }
        .codyncDialog("Something went wrong", isPresented: errorShown, message: errorMessage, cancel: "OK") { [] }
        .fullScreenCover(item: screenTarget) { target in
            if let store = accounts.store(for: target.computerId) {
                ScreenView(watching: target.request.watching)
                    .environment(store)
            }
        }
        .sheet(isPresented: Bindable(app).showComputers) {
            NavigationStack { SettingsView() }
        }
        .sheet(isPresented: Binding(get: { app.marketplace != nil }, set: { if !$0 { app.marketplace = nil } })) {
            if let store = app.marketplace.flatMap(accounts.store(for:)) {
                NavigationStack {
                    MarketplaceView { app.marketplace = nil }
                }
                .environment(store)
            }
        }
    }

    // MARK: one place for every computer's errors and screen requests

    private var errorMessage: String? {
        accounts.lastError ?? accounts.computers.lazy.compactMap { accounts.store(for: $0.id)?.lastError }.first
    }

    private var errorShown: Binding<Bool> {
        Binding(get: { errorMessage != nil }, set: { shown in
            guard !shown else { return }
            accounts.lastError = nil
            for computer in accounts.computers { accounts.store(for: computer.id)?.lastError = nil }
        })
    }

    private struct ScreenTarget: Identifiable {
        let computerId: ComputerID
        let request: ScreenRequest
        var id: String { "\(computerId)/\(request.id)" }
    }

    /// Any computer's "open the screen" request (thread toolbar, computers list, `codync://screen`).
    private var screenTarget: Binding<ScreenTarget?> {
        Binding(get: {
            accounts.computers.lazy.compactMap { c in
                accounts.store(for: c.id)?.screenRequest.map { ScreenTarget(computerId: c.id, request: $0) }
            }.first
        }, set: { target in
            guard target == nil else { return }
            for computer in accounts.computers { accounts.store(for: computer.id)?.screenRequest = nil }
        })
    }
}

/// A conversation, talking to the computer the bot lives on.
private struct ChatScreen: View {
    let ref: BotReference
    @Environment(AccountStore.self) private var accounts

    var body: some View {
        if let store = accounts.store(for: ref.computerId) {
            ThreadView(botId: ref.botId)
                .environment(store)
        } else {
            ContentUnavailableView("This computer was removed", systemImage: "desktopcomputer.trianglebadge.exclamationmark")
        }
    }
}

/// Usage for one computer at a time: the last active one, which the Usage widget shows too.
private struct UsageTab: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    /// Picked here; also becomes the computer the Usage widget shows.
    @State private var picked: ComputerID?

    var body: some View {
        if let store = picked.flatMap(accounts.store(for:)) ?? app.currentStore {
            UsageView()
                .environment(store)
                .toolbar {
                    if accounts.computers.count > 1 {
                        ToolbarItem(placement: .topBarTrailing) {
                            DropdownMenu {
                                accounts.computers.map { computer in
                                    MenuItem(computer.name, selected: computer.id == store.computer.id) {
                                        picked = computer.id
                                        accounts.storage.lastComputerId = computer.id
                                        WidgetCenter.shared.reloadTimelines(ofKind: "CodyncUsage")
                                    }
                                }
                            } label: {
                                ComputerBadge(store.computer, size: 28)
                            }
                            .accessibilityLabel("Computer: \(store.computer.name)")
                        }
                    }
                }
                .id(store.computer.id)
        } else {
            ContentUnavailableView("No computer yet", systemImage: "chart.bar",
                                   description: Text("Usage shows up once a computer is connected."))
        }
    }
}
