import CodyncKit
import CodyncUI
import SwiftUI
import WidgetKit

struct RootView: View {
    @Environment(AppStore.self) private var app
    @Environment(AccountStore.self) private var accounts
    @Environment(AccountSession.self) private var account
    @AppStorage("onboardingCompleted") private var onboardingCompleted = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// First launch only: past the welcome, on to setup.
    @State private var welcomed = false

    /// The open bot, as a NavigationStack path.
    private var path: Binding<[BotReference]> {
        Binding(get: { accounts.selection.map { [$0] } ?? [] }, set: { accounts.selection = $0.last })
    }

    var body: some View {
        Group {
            if accounts.computers.isEmpty && accounts.cloudComputers.isEmpty {
                if !onboardingCompleted && !welcomed && !account.isSignedIn {
                    WelcomeView { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { welcomed = true } }
                        .transition(.move(edge: .leading).combined(with: .opacity))
                } else {
                    PairingView {
                        if onboardingCompleted || account.isSignedIn {
                            AccountSwitcherButton()
                        } else {
                            BackButton { withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { welcomed = false } }
                        }
                    }
                    .transition(.move(edge: .trailing).combined(with: .opacity))
                }
            } else {
                tabs
            }
        }
        .background(Palette.background)
        .onChange(of: accounts.computers.isEmpty, initial: true) { _, empty in
            // Existing installations have already completed setup. Keep this
            // device-level milestone across account changes and unpairing.
            if !empty { onboardingCompleted = true }
        }
        .codyncDialog("Something went wrong", isPresented: errorShown, message: errorMessage, cancel: "OK") { [] }
        .codyncOverlay(isPresented: Binding(get: { screenTarget.wrappedValue != nil }, set: { if !$0 { screenTarget.wrappedValue = nil } })) { close in
            if let target = screenTarget.wrappedValue, let store = accounts.store(for: target.computerId) {
                ScreenView(watching: target.request.watching, close: close)
                    .environment(store)
                    .id(target.id)
            }
        }
        .codyncSheet(isPresented: Bindable(app).showComputers) {
            // Holds the pushes inside the sheet (Widgets, Live Activity); no bar shows.
            NavigationStack { SettingsView() }
        }
        .codyncSheet(isPresented: Binding(get: { app.marketplace != nil }, set: { if !$0 { app.marketplace = nil } })) {
            if let store = app.marketplace.flatMap(accounts.store(for:)) {
                MarketplaceView { app.marketplace = nil }
                    .environment(store)
            }
        }
    }

    /// Both tab stacks stay alive (scroll position, open thread); the tab bar hides while a thread is open.
    private var tabs: some View {
        ZStack {
            NavigationStack(path: path) {
                BotListView()
                    .navigationDestination(for: BotReference.self) { ref in ChatScreen(ref: ref) }
            }
            .tabLayer(app.tab == .bots)
            UsageTab()
                .tabLayer(app.tab == .usage)
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: app.tab)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if accounts.selection == nil {
                VStack(spacing: 8) {
                    AccessBanners()
                    TabBar(selection: Bindable(app).tab, tabs: [
                        (id: AppTab.bots, title: "Bots", icon: "bubble.left.and.bubble.right.fill"),
                        (id: AppTab.usage, title: "Usage", icon: "chart.bar.fill"),
                    ])
                    .padding(.bottom, 4)
                }
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: accounts.selection == nil)
        // Opening a bot (notification, widget, link) always lands on the Bots tab.
        .onChange(of: accounts.selection) { _, ref in if ref != nil { app.tab = .bots } }
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
            VStack(spacing: 0) {
                ScreenHeader { BackButton { accounts.selection = nil } } title: { EmptyView() } trailing: { EmptyView() }
                EmptyState(title: "This computer was removed", systemImage: "desktopcomputer.trianglebadge.exclamationmark")
            }
            .background(Palette.background)
            .hidesSystemNavigationBar()
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
        let store = picked.flatMap(accounts.store(for:)) ?? app.currentStore
        VStack(spacing: 0) {
            ScreenHeader {
                EmptyView()
            } title: {
                Text("Usage").font(.body.weight(.semibold)).foregroundStyle(Palette.text)
            } trailing: {
                if let store, accounts.computers.count > 1 {
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
            if let store {
                UsageView()
                    .environment(store)
                    .id(store.computer.id)
            } else {
                EmptyState(title: "No computer yet", systemImage: "chart.bar",
                           message: "Usage shows up once a computer is connected.")
            }
        }
        .background(Palette.background)
    }
}

/// A centered icon, title and optional line: what a screen shows when it has nothing yet.
struct EmptyState: View {
    let title: String
    let systemImage: String
    var message: String?

    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: systemImage).font(.system(size: 40)).foregroundStyle(Palette.tertiary)
            Text(title).font(.title3.weight(.semibold)).foregroundStyle(Palette.text)
            if let message {
                Text(message).font(.subheadline).foregroundStyle(Palette.secondary)
            }
        }
        .multilineTextAlignment(.center)
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// A page's title row inside a modal: the close button on the modal's first page,
/// a back button once pushed inside it.
struct PageHeader<Trailing: View>: View {
    let title: String
    var pushed = false
    let trailing: Trailing
    @Environment(\.dismiss) private var pop

    init(_ title: String, pushed: Bool = false, @ViewBuilder trailing: () -> Trailing) {
        self.title = title
        self.pushed = pushed
        self.trailing = trailing()
    }

    var body: some View {
        if pushed {
            ScreenHeader {
                BackButton { pop() }
            } title: {
                Text(title).font(.body.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(1)
            } trailing: {
                trailing
            }
        } else {
            ModalHeader(title) { trailing }
        }
    }
}

extension PageHeader where Trailing == EmptyView {
    init(_ title: String, pushed: Bool = false) { self.init(title, pushed: pushed) { EmptyView() } }
}

extension View {
    /// Tops a screen with its `PageHeader` in place of the system navigation bar.
    func page(_ title: String, pushed: Bool = false) -> some View {
        VStack(spacing: 0) {
            PageHeader(title, pushed: pushed)
            self
        }
        .background(Palette.background)
        .hidesSystemNavigationBar()
    }
}

private extension View {
    /// One tab's stack in the root ZStack: visible and interactive only while selected.
    func tabLayer(_ selected: Bool) -> some View {
        opacity(selected ? 1 : 0)
            .allowsHitTesting(selected)
            .accessibilityHidden(!selected)
    }
}

/// Account computers this iPhone asked on its own after sign-in: the code to check, until they answer.
private struct AccessBanners: View {
    @Environment(AccountStore.self) private var accounts

    var body: some View {
        VStack(spacing: 8) {
            ForEach(Array(accounts.pendingAccess.values).sorted { $0.requestId < $1.requestId }) { ticket in
                let name = accounts.cloudComputers.first { $0.computerId == ticket.computerId }?.name ?? "Your computer"
                HStack(spacing: 12) {
                    Spinner(size: 16)
                    VStack(alignment: .leading, spacing: 1) {
                        Text("Approve on \(name)").font(.subheadline.weight(.medium)).foregroundStyle(Palette.text)
                        Text("Code \(ticket.code.prefix(3)) \(ticket.code.suffix(3))")
                            .font(.footnote.monospacedDigit()).foregroundStyle(Palette.secondary)
                    }
                    Spacer(minLength: 8)
                    IconButton("Cancel request", systemImage: "xmark") {
                        Task { await accounts.cancelAccess(ticket.computerId) }
                    }
                }
                .padding(.leading, 14)
                .padding(.trailing, 6)
                .padding(.vertical, 8)
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .padding(.horizontal, 16)
        .animation(Motion.layout, value: accounts.pendingAccess.keys.sorted())
        .accessibilityElement(children: .contain)
    }
}
