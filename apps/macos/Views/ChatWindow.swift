import CodyncKit
import CodyncUI
import SwiftUI

/// Native desktop chat: roster on the left, the conversation on the right
/// (Grok Bot's desktop layout), sharing its views with the iPhone app.
struct ChatWindow: View {
    @Environment(HostController.self) private var host
    @Environment(AccountSession.self) private var account
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage("macAccountOnboardingCompleted") private var accountOnboardingCompleted = false

    private var showsChat: Bool {
        // SSH computers attach only after connecting. Keep the roster and connection
        // controls available while a saved remote connects or needs attention.
        host.state == .running || !host.accounts.computers.isEmpty
            || !host.accounts.cloudComputers.isEmpty || !host.ssh.profiles.isEmpty
    }

    var body: some View {
        ZStack {
            if account.isSignedIn || accountOnboardingCompleted {
                if showsChat {
                    ChatSplitView().id(host.contextID)
                        .transition(.opacity)
                } else {
                    hostState
                        .transition(.opacity.combined(with: .scale(scale: 0.97)))
                }
            } else if !account.isReady {
                AccountRestoreView(onContinue: { accountOnboardingCompleted = true })
                    .transition(.opacity)
            } else {
                AccountWelcomeView(
                    isConfigured: account.isConfigured,
                    isBusy: account.isBusy,
                    errorMessage: account.errorMessage,
                    onContinue: { accountOnboardingCompleted = true },
                    onSignIn: { provider in
                        Task { await account.signIn(provider: provider) }
                    }
                )
                .transition(.opacity)
            }
        }
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: showsChat)
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: host.state)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: accountOnboardingCompleted)
        .onChange(of: account.isSignedIn) { _, signedIn in
            if signedIn { accountOnboardingCompleted = true }
        }
        .task {
            if account.isSignedIn { accountOnboardingCompleted = true }
        }
        .frame(minWidth: 760, minHeight: 500)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Palette.background)
        .tint(Palette.accent)
        .ignoresSafeArea(.container, edges: .top)
        .codyncDialog("Reset all data?", isPresented: Bindable(host).confirmsReset,
                      message: "Signs out every account and deletes this Mac's bots, conversations, keys and settings. Codync then starts over from the welcome screen.") {
            [DialogAction("Reset everything", destructive: true) { Task { await host.resetAllData() } }]
        }
        // Approving a device: the code the device shows must match (spec §4.2 B).
        // It closes when the request is decided or put off (both change `currentApproval`);
        // closing it puts the request off.
        .codyncSheet(isPresented: Binding(
            get: { host.currentApproval != nil },
            set: { if !$0, let approval = host.currentApproval { host.deferApproval(approval) } }
        )) {
            CurrentApprovalSheet()
        }
    }
}

/// Keep first-run sign-in choices behind Clerk's persisted-session restoration on launch.
private struct AccountRestoreView: View {
    let onContinue: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            Spinner(size: 24)
            Text("Checking for a saved account…")
                .appFont(.callout)
                .foregroundStyle(Palette.secondary)
            Button("Continue on this Mac", action: onContinue)
                .buttonStyle(.plain)
                .foregroundStyle(Palette.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// First-run account choice. Existing Clerk sessions skip this screen as soon as Clerk restores them.
private struct AccountWelcomeView: View {
    let isConfigured: Bool
    let isBusy: Bool
    let errorMessage: String?
    let onContinue: () -> Void
    let onSignIn: (AccountSession.SignInProvider) -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var beat = 0

    var body: some View {
        GeometryReader { geometry in
            ScrollView {
                content
                    .frame(minHeight: geometry.size.height)
                    .frame(maxWidth: 480)
                    .frame(maxWidth: .infinity)
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .background(Palette.background)
        .task {
            if reduceMotion { beat = 4; return }
            for next in 1...4 {
                try? await Task.sleep(for: .milliseconds(next == 1 ? 150 : 280))
                withAnimation(.spring(duration: 0.6, bounce: 0.25)) { beat = next }
            }
        }
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: 0) {
            Spacer(minLength: 24)

            WelcomeCrew(shown: beat >= 1)
                .frame(maxWidth: .infinity)

            Spacer(minLength: 24)

            WelcomeChatGlimpse(shown: beat >= 2)
                .padding(.bottom, 24)

            VStack(alignment: .leading, spacing: 10) {
                Text("Your coding agents,\nas teammates.")
                    .appFont(.system(size: 34, weight: .semibold))
                    .tracking(-0.6)
                    .foregroundStyle(Palette.text)
                    .fixedSize(horizontal: false, vertical: true)
                Text("Give each one a name and a project. They work on your computer while you're away.")
                    .appFont(.body)
                    .foregroundStyle(Palette.secondary)
            }
            .opacity(beat >= 3 ? 1 : 0)
            .offset(y: beat >= 3 ? 0 : 14)

            Spacer(minLength: 32)

            VStack(spacing: 10) {
                Button(action: onContinue) {
                    Text("Continue on this Mac")
                        .appFont(.headline)
                        .frame(maxWidth: .infinity, minHeight: 50)
                }
                .buttonStyle(.primary)

                Button { onSignIn(.apple) } label: {
                    ZStack {
                        HStack(spacing: 10) {
                            Image(systemName: "apple.logo").appFont(.system(size: 20))
                            Text("Continue with Apple")
                        }
                        .opacity(isBusy ? 0 : 1)
                        if isBusy { Spinner(size: 18) }
                    }
                    .appFont(.headline)
                    .frame(maxWidth: .infinity, minHeight: 50)
                }
                .buttonStyle(.primary)
                .disabled(!isConfigured || isBusy)

                Button { onSignIn(.google) } label: {
                    ZStack {
                        HStack(spacing: 10) {
                            Image("google")
                                .resizable()
                                .scaledToFit()
                                .frame(width: 20, height: 20)
                            Text("Continue with Google")
                        }
                        .opacity(isBusy ? 0 : 1)
                        if isBusy { Spinner(size: 18) }
                    }
                    .appFont(.headline)
                    .frame(maxWidth: .infinity, minHeight: 50)
                }
                .buttonStyle(.secondary)
                .disabled(!isConfigured || isBusy)

                if !isConfigured {
                    Text("Sign-in isn't configured in this build. You can still use Codync on this Mac.")
                        .appFont(.footnote)
                        .foregroundStyle(Palette.tertiary)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let errorMessage {
                    Text(errorMessage)
                        .appFont(.footnote)
                        .foregroundStyle(Palette.warning)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            .opacity(beat >= 4 ? 1 : 0)
            .offset(y: beat >= 4 ? 0 : 14)
        }
        .padding(.horizontal, 24)
        .padding(.bottom, 12)
    }
}

/// The animated bot group from the first iPhone onboarding screen.
private struct WelcomeCrew: View {
    let shown: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private let members: [(shape: String, color: String, size: CGFloat, x: CGFloat, y: CGFloat, mood: CharacterAvatar.Mood)] = [
        ("blob", "blue", 92, 0, 0, .working),
        ("squircle", "orange", 64, -104, -34, .idle),
        ("teardrop", "violet", 58, 100, -46, .working),
        ("hex", "green", 48, -76, 64, .idle),
        ("cloud", "magenta", 52, 84, 58, .needsInput),
    ]

    var body: some View {
        TimelineView(.animation(paused: reduceMotion || !shown)) { context in
            let time = context.date.timeIntervalSinceReferenceDate
            ZStack {
                ForEach(members.indices, id: \.self) { index in
                    let member = members[index]
                    CharacterAvatar(shape: member.shape, color: member.color, size: member.size, mood: member.mood)
                        .offset(x: member.x, y: member.y + (reduceMotion ? 0 : sin(time * 1.3 + Double(index) * 1.7) * 5))
                        .scaleEffect(shown ? 1 : 0.3)
                        .opacity(shown ? 1 : 0)
                        .animation(.spring(duration: 0.7, bounce: 0.4).delay(Double(index) * 0.07), value: shown)
                }
            }
        }
        .frame(height: 170)
        .accessibilityHidden(true)
    }
}

/// A short animated user and bot exchange, matching the iPhone onboarding preview.
private struct WelcomeChatGlimpse: View {
    let shown: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var replied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Fix the flaky login test")
                .appFont(.subheadline)
                .foregroundStyle(Palette.text)
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 18))
                .frame(maxWidth: .infinity, alignment: .trailing)
                .opacity(shown ? 1 : 0)
                .offset(y: shown ? 0 : 14)

            HStack(alignment: .bottom, spacing: 8) {
                CharacterAvatar(shape: "blob", color: "blue", size: 28, mood: replied ? .idle : .working)
                if replied {
                    Text("Done. It raced the session refresh; tests pass on fix/login.")
                        .appFont(.subheadline)
                        .foregroundStyle(Palette.text)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 9)
                        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 18))
                        .transition(.opacity.combined(with: .scale(scale: 0.92, anchor: .bottomLeading)))
                } else {
                    Text("Working…")
                        .appFont(.subheadline)
                        .foregroundStyle(Palette.tertiary)
                        .transition(.opacity)
                }
            }
            .opacity(shown ? 1 : 0)
            .offset(y: shown ? 0 : 14)
        }
        .accessibilityElement(children: .combine)
        .onChange(of: shown) { _, isShown in
            guard isShown else { return }
            Task {
                try? await Task.sleep(for: .seconds(reduceMotion ? 0 : 1.6))
                withAnimation(Motion.reduced(.spring(duration: 0.45, bounce: 0.2), reduceMotion)) { replied = true }
            }
        }
    }
}

/// What stands between this Mac and the chat, with the one action that fixes it.
private extension ChatWindow {
    @ViewBuilder var hostState: some View {
        switch host.state {
        case .notInstalled:
            EmptyState(
                icon: "desktopcomputer",
                title: "Set up this Mac",
                message: "Codync runs your coding agents through the host, a small background service on this Mac.",
                action: ("Install host", host.install)
            )
        case .missingBinary:
            EmptyState(
                icon: "desktopcomputer.trianglebadge.exclamationmark",
                title: "The host is missing",
                message: "This copy of Codync doesn't include codync-host. Download Codync again from codync.dev or GitHub."
            )
        case .starting, .running:
            EmptyState(icon: "desktopcomputer", title: "Starting the host…", message: "This takes a few seconds.")
        case let .failed(message):
            EmptyState(
                icon: "desktopcomputer.trianglebadge.exclamationmark",
                title: "The host isn't running",
                message: message,
                action: ("Try again", host.restart)
            )
        }
    }
}

/// The request being decided, swapped in place when the next one comes up; keeps the last
/// one on screen while the card animates away.
private struct CurrentApprovalSheet: View {
    @Environment(HostController.self) private var host
    @State private var last: Approval?

    var body: some View {
        if let approval = host.currentApproval ?? last {
            ApprovalSheet(approval: approval)
                .id(approval.id)
                .onAppear { last = approval }
        }
    }
}

/// A quiet placeholder for an empty screen: icon, title, a line of help and an optional action.
struct EmptyState: View {
    let icon: String
    let title: String
    let message: String
    var action: (title: String, run: () -> Void)?

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: icon)
                .appFont(.system(size: 36, weight: .light))
                .foregroundStyle(Palette.tertiary)
                .accessibilityHidden(true)
            Text(title).appFont(.title3.weight(.semibold)).foregroundStyle(Palette.text)
            Text(message)
                .appFont(.callout)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 360)
            if let action {
                Button(action.title, action: action.run).buttonStyle(.primary).padding(.top, 4)
            }
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// A bot together with the store of the computer it lives on.
@MainActor
private struct BotTarget {
    let bot: Bot
    let store: BotStore
}

private struct EditTarget: Identifiable {
    let request: EditorRequest
    let store: BotStore
    var id: UUID { request.id }
}

private struct ChatSplitView: View {
    @Environment(HostController.self) private var host
    @Environment(AccountSession.self) private var account
    @Environment(UpdatesManager.self) private var updates
    @State private var editing: EditTarget?
    @State private var confirmDelete: BotTarget?
    @State private var contextBot: BotTarget?
    @State private var contextPoint = CGPoint.zero
    @State private var newSessionBot: BotTarget?
    @State private var composing = false
    @State private var newMenu = false
    @State private var railNewMenu = false
    /// The compose page gathers a group chat ("New group chat") rather than any chat.
    @State private var composingGroup = false
    /// The computer a new chat goes to.
    @State private var composeComputer: ComputerID?
    @State private var previousSelection: BotReference?
    @State private var hoveredBot: BotReference?
    @State private var marketplace: ComputerID?
    @State private var showComputers = false
    @State private var search = ""
    @State private var showUsage = false
    @State private var showAccount = false
    @State private var hoveredFooter: String?
    @FocusState private var searchFocused: Bool
    @FocusState private var profileFocused: Bool
    @AppStorage("hiddenComputers") private var hiddenComputers = ""
    @AppStorage("sidebarCompact") private var compact = false
    @AppStorage("desktopSidebarWidth") private var sidebarWidth = 296.0
    @State private var dragStartWidth: Double?
    @State private var appUpdates = AppUpdates()
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Sheets hang from the window's title bar; sized from it so they never run past its bottom edge.
    @State private var windowSize = CGSize(width: 1100, height: 760)

    private var sheetHeight: CGFloat { max(420, windowSize.height - 76) }

    private var accounts: AccountStore { host.accounts }
    /// Every computer with a store, in the account's order.
    private var stores: [BotStore] { accounts.computers.compactMap { accounts.store(for: $0.id) } }
    /// Computers that can take a new chat: shown, online, and on versions that work together.
    private var onlineStores: [BotStore] { stores.filter { shownIDs.contains($0.computer.id) && $0.connection == .online && $0.mismatch == nil } }
    private var selectedStore: BotStore? { accounts.selection.flatMap { accounts.store(for: $0.computerId) } }
    private var composeStore: BotStore? { composeComputer.flatMap { accounts.store(for: $0) } }
    private var shownIDs: Set<ComputerID> {
        Set(ComputerSelection(all: accounts.computers.map(\.id), hidden: hiddenComputers).shown)
    }
    private var connectingComputers: [ComputerConnectionProgress] {
        host.ssh.profiles.compactMap { profile in
            let detail: String
            switch host.ssh.status(of: profile.id) {
            case let .connecting(step): detail = step
            case .retrying: detail = "Retrying…"
            default: return nil
            }
            if let id = profile.computerId, accounts.computers.contains(where: { $0.id == id }), !shownIDs.contains(id) { return nil }
            return ComputerConnectionProgress(computerId: profile.computerId,
                                              name: profile.name.isEmpty ? profile.host : profile.name, detail: detail)
        }
    }
    private var isConnecting: Bool {
        onlineStores.isEmpty && (!connectingComputers.isEmpty
            || stores.contains { shownIDs.contains($0.computer.id) && $0.shownConnection == .connecting })
    }
    private var visibleRoster: [RosterItem] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        return accounts.roster.filter { item in
            shownIDs.contains(item.ref.computerId) && (compact || query.isEmpty
                || item.bot.name.localizedCaseInsensitiveContains(query)
                || (item.bot.lastMessage?.localizedCaseInsensitiveContains(query) ?? false))
        }
    }

    private func ref(_ bot: Bot, _ store: BotStore) -> BotReference {
        BotReference(accountId: accounts.accountId, computerId: store.computer.id, botId: bot.id)
    }

    /// Kit views (new chat, bot editor) select a new bot on their own store; that becomes the account's selection.
    private var storeSelection: BotReference? {
        stores.lazy.compactMap { store in store.selection.map { BotReference(accountId: accounts.accountId, computerId: store.computer.id, botId: $0) } }.first
    }

    var body: some View {
        HStack(spacing: 0) {
            VStack(spacing: 0) {
                HStack(spacing: 6) {
                    Spacer(minLength: 0)
                    ComputerFilterHeader(accounts: accounts, hidden: $hiddenComputers,
                                         connectingComputers: connectingComputers) { showComputers = true }
                    IconButton("New", systemImage: "plus") { newMenu.toggle() }
                        .codyncMenu(isPresented: $newMenu, items: newItems)
                        .disabled(onlineStores.isEmpty)
                }
                .padding(.leading, compact ? 0 : 80)
                .padding(.trailing, 18)
                .frame(height: 44)
                .opacity(compact ? 0 : 1)
                .allowsHitTesting(!compact)
                .accessibilityHidden(compact)
                if compact {
                    ComputerFilterHeader(accounts: accounts, hidden: $hiddenComputers, compact: true,
                                         connectingComputers: connectingComputers) { showComputers = true }
                        .frame(width: 70)
                }
                ScrollView {
                    LazyVStack(spacing: 2) {
                        if !compact {
                            // Other computers on older releases (this Mac's own host is the app's).
                            UpdateReminders(stores: stores.filter { shownIDs.contains($0.computer.id) })
                            ForEach(stores.filter { shownIDs.contains($0.computer.id) }, id: \.computer.id) { store in
                                if let mismatch = store.mismatch {
                                    UpdateNeededCard(store: store, mismatch: mismatch)
                                        .padding(.bottom, 8)
                                        .transition(.opacity)
                                }
                            }
                        }
                        ForEach(visibleRoster) { item in
                            if let store = accounts.store(for: item.ref.computerId) {
                                row(item.bot, store)
                            }
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.top, 4)
                }
                .onMoveCommand { direction in
                    let refs = visibleRoster.map(\.ref)
                    guard direction == .up || direction == .down, !refs.isEmpty else { return }
                    let current = refs.firstIndex { $0 == accounts.selection } ?? -1
                    let next = direction == .down ? min(current + 1, refs.count - 1) : max(current - 1, 0)
                    accounts.selection = refs[next]
                }
                .scrollContentBackground(.hidden)
                .background(Palette.surface)
                .safeAreaInset(edge: .top, spacing: 0) {
                    if !compact {
                        VStack(spacing: 4) {
                            searchField
                        }
                        .frame(width: sidebarWidth)
                        .transition(.opacity)
                    }
                }
                .overlay {
                    // A computer waiting for an update shows its card instead; its bots aren't known yet.
                    if visibleRoster.isEmpty && !compact
                        && !stores.contains(where: { shownIDs.contains($0.computer.id) && $0.mismatch != nil }) {
                        VStack(spacing: 8) {
                            Text(search.isEmpty ? (isConnecting ? "Connecting…" : "No bots yet") : "No matching bots")
                                .appFont(.system(size: 13, weight: .medium))
                            Text(search.isEmpty ? (isConnecting ? "Your bots will appear once connected." : "Use + to start a new chat.") : "Try another name or message.")
                                .appFont(.system(size: 12)).foregroundStyle(Palette.secondary)
                                .multilineTextAlignment(.center)
                        }
                        .padding(.horizontal, 16)
                        .allowsHitTesting(false)
                    }
                }
                .safeAreaInset(edge: .bottom) {
                    ZStack(alignment: .bottom) {
                        if compact {
                            railActions
                                .frame(width: 76)
                                .transition(.opacity)
                        } else {
                            sidebarFooter
                                .frame(width: sidebarWidth)
                                .transition(.opacity)
                        }
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .frame(width: compact ? 76 : sidebarWidth)
            .frame(maxHeight: .infinity)
            .background(Palette.surface)
            .clipped()
            sidebarDivider
            Group {
                if composing, let store = composeStore {
                    VStack(spacing: 0) {
                        if onlineStores.count > 1 { computerPicker }
                        // The To: row takes the title bar's place instead of sitting under an empty one.
                        NewChatView(group: composingGroup) {
                            composing = false
                            if accounts.selection == nil { accounts.selection = previousSelection }
                        }
                        .environment(store)
                        .id("\(store.computer.id)-\(composingGroup)")
                    }
                    .ignoresSafeArea(.container, edges: .top)

                } else if let ref = accounts.selection, let store = selectedStore, store.bots[ref.botId] != nil {
                    ThreadView(botId: ref.botId)
                        .environment(store)
                        .id(ref)
                        .transition(.asymmetric(insertion: .opacity, removal: .identity))
                } else if isConnecting && visibleRoster.isEmpty {
                    VStack(spacing: 14) {
                        Spinner(size: 24)
                        Text("Connecting to your computer…").font(.title2.weight(.semibold))
                        Text("Your bots will appear once connected.").foregroundStyle(Palette.secondary)
                        Button("Manage computers") { showComputers = true }.buttonStyle(.secondary)
                    }
                    .padding(32)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    VStack(spacing: 14) {
                        HStack(spacing: -10) {
                            CharacterAvatar(shape: "blob", color: "blue", size: 60, mood: .working)
                            CharacterAvatar(shape: "squircle", color: "orange", size: 60)
                            CharacterAvatar(shape: "teardrop", color: "violet", size: 60, mood: .working)
                        }
                        Text("Your coding agents, as teammates.").appFont(.title2.weight(.semibold))
                        Text("Pick a bot, or create one for each kind of work and point it at a project.")
                            .foregroundStyle(Palette.secondary)
                            .multilineTextAlignment(.center)
                            .frame(maxWidth: 420)
                        Button("New Bot") { compose() }
                            .buttonStyle(.primary)
                            .disabled(onlineStores.isEmpty)
                    }
                    .padding(32)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
        }
        // Animate mode changes from every entry point, while ordinary resizing tracks the pointer.
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: compact)
        .coordinateSpace(name: "chat-window")
        .allowsHitTesting(!showAccount && contextBot == nil)
        .accessibilityHidden(showAccount || contextBot != nil)
        .overlay(alignment: .bottomLeading) {
            if showAccount {
                ZStack(alignment: .bottomLeading) {
                    Color.black.opacity(0.001)
                        .contentShape(Rectangle())
                        .onTapGesture { dismissAccountMenu() }
                        .accessibilityLabel("Dismiss account menu")
                        .accessibilityAddTraits(.isButton)
                    SidebarAccountPanel(
                        name: localProfileName,
                        signedIn: account.isSignedIn,
                        email: account.email,
                        busy: account.isBusy,
                        errorMessage: account.errorMessage ?? updates.errorMessage,
                        onSignIn: { provider in
                            dismissAccountMenu()
                            Task {
                                await account.signIn(provider: provider)
                                if account.errorMessage != nil {
                                    withAnimation(Motion.fade) { showAccount = true }
                                }
                            }
                        },
                        onSignOut: {
                            dismissAccountMenu()
                            Task {
                                await host.signOut()
                                if account.errorMessage != nil {
                                    withAnimation(Motion.fade) { showAccount = true }
                                }
                            }
                        },
                        compact: compact,
                        approvals: host.approvals.count,
                        updateVersion: updates.availableVersion,
                        canUpdate: updates.canCheckForUpdates || updates.hasStagedUpdate,
                        onDismiss: dismissAccountMenu,
                        onUsage: { dismissAccountMenu(); showUsage = true },
                        onComputers: { dismissAccountMenu(); showComputers = true },
                        onUpdate: { dismissAccountMenu(); updates.checkForUpdates() },
                        onToggleSidebar: {
                            dismissAccountMenu()
                            compact.toggle()
                        },
                        onSearch: { dismissAccountMenu(); compact = false; searchFocused = true }
                    )
                    .frame(width: min(260, windowSize.width - 32))
                    .padding(.leading, 16)
                    .padding(.bottom, compact ? 68 : 64)
                    .transition(.scale(scale: 0.97, anchor: .bottomLeading).combined(with: .opacity))
                }
            }
        }
        .overlay(alignment: .topLeading) {
            if let bot = contextBot {
                ZStack(alignment: .topLeading) {
                    Color.black.opacity(0.001)
                        .contentShape(Rectangle())
                        .onTapGesture { contextBot = nil }
                        .accessibilityLabel("Dismiss conversation menu")
                        .accessibilityAddTraits(.isButton)
                    DesktopActionMenu(items: botActions(bot.bot, bot.store), onDismiss: { contextBot = nil })
                        .frame(width: 260)
                        .offset(x: min(max(8, contextPoint.x), windowSize.width - 268),
                                y: min(max(8, contextPoint.y), max(8, windowSize.height - 344)))
                        .transition(.opacity)
                }
            }
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: showAccount)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: contextBot?.bot.id)
        .ignoresSafeArea(.container, edges: .top)
        .codyncSheet(item: $editing) { target in
            BotEditorView(draft: target.request.draft)
                .environment(target.store)
                .frame(width: 520, height: min(680, sheetHeight))
        }
        .codyncSheet(isPresented: Binding(get: { marketplace != nil }, set: { if !$0 { marketplace = nil } })) {
            if let store = marketplace.flatMap(accounts.store(for:)) {
                MarketplaceView(computers: onlineStores.map { ($0.computer.id, $0.hostName) },
                                computer: Binding(get: { store.computer.id }, set: { marketplace = $0 }))
                    .environment(store)
                    .id(store.computer.id)
                .frame(width: min(920, windowSize.width - 80), height: sheetHeight)
            }
        }
        .codyncSheet(isPresented: $showComputers) {
            ComputersView()
                .frame(width: 620, height: sheetHeight)
        }
        .codyncSheet(isPresented: $showUsage) {
            UsageSheet().frame(width: 520)
        }
        .background {
            collapseButton.hidden()
            Button("New chat") { compose() }
                .keyboardShortcut("n")
                .disabled(onlineStores.isEmpty)
                .hidden()
            Button("Search bots") {
                compact = false
                searchFocused = true
            }
            .keyboardShortcut("f")
            .hidden()
        }
        .onGeometryChange(for: CGSize.self) {
            $0.size
        } action: {
            windowSize = $0
            sidebarWidth = min(sidebarWidth, max(260, $0.width - 420))
        }
        .hiddenWindowTitle()
        // A computer and this app on versions that don't work together: Sparkle updates the app;
        // this Mac's own host is put back on the app's bundled copy.
        .environment(\.appUpdate, updates.canCheckForUpdates || updates.hasStagedUpdate
            ? AppUpdateAction { [updates] in updates.checkForUpdates() } : nil)
        .environment(\.hostUpdate, HostUpdateAction(available: { [host] in $0 === host.store }) { [host] _ in host.restart() })
        // No App Store lookup here (Sparkle updates the Mac app); keeps reminder dismissals.
        .environment(appUpdates)
        .codyncDialog("Start a new session?", isPresented: Binding(
            get: { newSessionBot != nil },
            set: { if !$0 { newSessionBot = nil } }
        ), message: "The conversation stays here, but the agent starts with a fresh context.") {
            guard let target = newSessionBot else { return [] }
            return [DialogAction("New session") { target.store.newSession(target.bot.id); newSessionBot = nil }]
        }
        .codyncDialog("Delete \(confirmDelete?.bot.name ?? "bot")?", isPresented: Binding(
            get: { confirmDelete != nil },
            set: { if !$0 { confirmDelete = nil } }
        ), message: "Files it changed on your computer stay as they are.") {
            guard let target = confirmDelete else { return [] }
            return [DialogAction("Delete bot and its conversation", destructive: true) { target.store.delete(target.bot) }]
        }
        .codyncDialog("Something went wrong", isPresented: Binding(get: { errorMessage != nil }, set: { if !$0 { clearErrors() } }),
                      message: errorMessage, cancel: "OK") { [] }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: accounts.selection)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: composing)
        .onChange(of: accounts.selection) { _, ref in if ref != nil { composing = false } }
        .onChange(of: storeSelection) { _, ref in
            guard let ref else { return }
            accounts.store(for: ref.computerId)?.selection = nil
            accounts.selection = ref
        }
        #if DEBUG
            .onAppear {
                // Screenshot/UI checks: CODYNC_DEBUG_OPEN=compose | group | plugins | computers | <bot name>
                let target = ProcessInfo.processInfo.environment["CODYNC_DEBUG_OPEN"]
                if target == "compose" || target == "group" {
                    compose(group: target == "group")
                } else if target == "plugins" {
                    marketplace = host.store?.computer.id
                } else if target == "computers" {
                    showComputers = true
                } else if let target, let item = accounts.roster.first(where: { $0.bot.name == target }) {
                    accounts.selection = item.ref
                }
            }
        #endif
    }
}

extension ChatSplitView {
    fileprivate var sidebarDivider: some View {
        Rectangle()
            .fill(Palette.border)
            .frame(width: 1)
            .overlay {
                Color.clear.frame(width: 9)
                    .contentShape(Rectangle())
                    .gesture(
                        DragGesture(minimumDistance: 0, coordinateSpace: .named("chat-window"))
                            .onChanged { value in
                                if dragStartWidth == nil { dragStartWidth = compact ? 76 : sidebarWidth }
                                let proposed = (dragStartWidth ?? 296) + value.translation.width
                                compact = proposed < 180
                                if !compact {
                                    sidebarWidth = min(max(260, proposed), min(420, windowSize.width - 420))
                                }
                            }
                            .onEnded { _ in dragStartWidth = nil }
                    )
                    .onHover { hovering in
                        if hovering { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
                    }
            }
            .accessibilityLabel("Resize sidebar")
            .accessibilityValue("\(compact ? 76 : Int(sidebarWidth)) points")
            .accessibilityAdjustableAction { direction in
                switch direction {
                case .increment:
                    if compact { compact = false }
                    else { sidebarWidth = min(sidebarWidth + 20, min(420, windowSize.width - 420)) }
                case .decrement:
                    if sidebarWidth <= 260 { compact = true }
                    else { sidebarWidth = max(260, sidebarWidth - 20) }
                @unknown default: break
                }
            }
    }

    fileprivate var searchField: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass").foregroundStyle(Palette.secondary)
            TextField("Search", text: $search)
                .textFieldStyle(.plain)
                .focused($searchFocused)
                .accessibilityLabel("Search bots")
                .onKeyPress(.escape) {
                    search = ""
                    searchFocused = false
                    return .handled
                }
            if !search.isEmpty {
                Button("Clear search", systemImage: "xmark.circle.fill") { search = "" }
                    .labelStyle(.iconOnly).buttonStyle(.plain)
                    .foregroundStyle(Palette.secondary)
                    .transition(.opacity.combined(with: .scale(scale: 0.8)))
            }
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: search.isEmpty)
        .appFont(.system(size: 13))
        .padding(.horizontal, 9)
        .frame(height: 28)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 12)
        .padding(.top, 4)
        .padding(.bottom, 8)
        .background(Palette.surface)
    }

    private var localProfileName: String { "Account" }

    private func row(_ bot: Bot, _ store: BotStore) -> some View {
        let ref = ref(bot, store)
        return Button {
            accounts.selection = ref
        } label: {
            BotRow(bot: bot, compact: compact)
                .environment(store)
                .padding(.horizontal, compact ? 0 : 8)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(
                    accounts.selection == ref
                        ? Palette.bubbleUser : hoveredBot == ref ? Palette.bubbleAgent : Color.clear,
                    in: RoundedRectangle(cornerRadius: 10)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(stores.count > 1 ? "\(bot.name), on \(store.hostName)" : bot.name)
        .accessibilityAddTraits(accounts.selection == ref ? .isSelected : [])
        .help(stores.count > 1 ? "\(bot.name) · \(store.hostName)" : bot.name)
        .onHover { hoveredBot = $0 ? ref : nil }
        .overlay {
            GeometryReader { geometry in
                SecondaryClickCapture { point in
                    let frame = geometry.frame(in: .named("chat-window"))
                    contextPoint = CGPoint(x: frame.minX + point.x, y: frame.minY + point.y)
                    contextBot = BotTarget(bot: bot, store: store)
                }
            }
        }
        .accessibilityAction(named: "Conversation actions") {
            contextPoint = CGPoint(x: compact ? 64 : sidebarWidth - 24, y: 100)
            contextBot = BotTarget(bot: bot, store: store)
        }
    }

    /// Which computer a new chat goes to, when more than one is online.
    fileprivate var computerPicker: some View {
        HStack(spacing: 8) {
            Text("On").foregroundStyle(Palette.secondary)
            ChoicePicker(selection: Binding(get: { composeComputer ?? "" }, set: { composeComputer = $0 }),
                         options: onlineStores.map { ($0.computer.id, $0.hostName) })
            Spacer()
        }
        .appFont(.callout)
        .padding(.horizontal, 22)
        .padding(.top, 12)
    }

    private var errorMessage: String? {
        accounts.lastError ?? stores.lazy.compactMap(\.lastError).first
    }

    private func clearErrors() {
        accounts.lastError = nil
        for store in stores { store.lastError = nil }
    }

    private var profileAvatar: some View {
        Group {
            if let url = account.avatarURL {
                AsyncImage(url: url) { image in image.resizable().scaledToFill() } placeholder: { profilePlaceholder }
            } else { profilePlaceholder }
        }
        .frame(width: 28, height: 28)
        .background(Palette.bubbleUser, in: Circle())
        .clipShape(Circle())
        .overlay { if account.isBusy { Spinner(size: 14) } }
        .accessibilityHidden(true)
    }

    private var profilePlaceholder: some View {
        Image(systemName: "person.fill")
            .appFont(.system(size: 15, weight: .medium))
            .foregroundStyle(Palette.secondary)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    fileprivate var sidebarFooter: some View {
        VStack(spacing: 4) {
            Button {
                marketplace = (selectedStore ?? host.store ?? onlineStores.first)?.computer.id
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "square.grid.2x2")
                        .appFont(.system(size: 14, weight: .regular))
                        .frame(width: 30, height: 30)
                        .background(Palette.text.opacity(0.025), in: Circle())
                    Text("Marketplace")
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 8)
                .frame(height: 36)
                .background(hoveredFooter == "marketplace" ? Palette.bubbleAgent : .clear,
                            in: RoundedRectangle(cornerRadius: 10))
                .contentShape(Rectangle())
            }
            .onHover { hoveredFooter = $0 ? "marketplace" : nil }

            Button {
                showAccount.toggle()
            } label: {
                HStack(spacing: 10) {
                    profileAvatar
                    Text(localProfileName)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 8)

                }
                .padding(.horizontal, 8)
                .frame(height: 38)
                .background(hoveredFooter == "account" || showAccount ? Palette.bubbleAgent : .clear,
                            in: RoundedRectangle(cornerRadius: 10))
                .contentShape(Rectangle())
            }
            .onHover { hoveredFooter = $0 ? "account" : nil }
            .accessibilityLabel("Open account menu for \(localProfileName)")
            .help("Account menu")
            .focused($profileFocused)
        }
        .buttonStyle(.plain)
        .appFont(.system(size: 13, weight: .medium))
        .foregroundStyle(Palette.text)
        .padding(.horizontal, 12)
        .padding(.top, 6)
        .padding(.bottom, 10)
        .background(Palette.surface)
    }

    /// The "+" menu: a chat with one or more bots, or a group chat. Both use the To: page.
    fileprivate func newItems() -> [MenuItem] {
        [MenuItem("New chat", icon: "square.and.pencil") { compose() },
         MenuItem("New group chat", icon: "person.2") { compose(group: true) }]
    }

    fileprivate func compose(group: Bool = false) {
        composingGroup = group
        // The selected bot's computer if it's online, else this Mac, else any online one.
        let target = [selectedStore, host.store].compactMap { $0 }.first { store in onlineStores.contains { $0 === store } } ?? onlineStores.first
        guard let target else { return }
        if !composing { previousSelection = accounts.selection }
        composeComputer = target.computer.id
        accounts.selection = nil
        composing = true
    }

    fileprivate var collapseButton: some View {
        Button(compact ? "Expand Sidebar" : "Collapse Sidebar", systemImage: "sidebar.left") {
            compact.toggle()
        }
        .keyboardShortcut("s", modifiers: [.control, .command])
        .help(compact ? "Expand sidebar" : "Collapse sidebar")
    }

    private func dismissAccountMenu() {
        showAccount = false
        profileFocused = true
    }

    /// The compact rail keeps the same destinations as the expanded footer.
    fileprivate var railActions: some View {
        VStack(spacing: 4) {
            Button("New", systemImage: "plus") { railNewMenu.toggle() }
                .disabled(onlineStores.isEmpty)
                .help("New")
                .labelStyle(.iconOnly)
                .buttonStyle(IconButtonStyle(size: 36))
                .codyncMenu(isPresented: $railNewMenu, items: newItems)
            Button("Marketplace", systemImage: "square.grid.2x2") {
                marketplace = (selectedStore ?? host.store ?? onlineStores.first)?.computer.id
            }
                .help("Marketplace")
                .labelStyle(.iconOnly)
                .buttonStyle(IconButtonStyle(size: 36))
            Button { showAccount.toggle() } label: {
                profileAvatar
                    .frame(width: 44, height: 44)
                    .background(showAccount ? Palette.bubbleAgent : .clear, in: RoundedRectangle(cornerRadius: 12))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .focused($profileFocused)
            .accessibilityLabel("Open account menu for \(localProfileName)")
            .help(localProfileName)
            .padding(.top, 4)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 8)
        .padding(.bottom, 10)
    }

    private func botActions(_ bot: Bot, _ model: BotStore) -> [DesktopActionMenu.Item] {
        let target = BotTarget(bot: bot, store: model)
        return [
            .init(title: bot.pinned ? "Unpin" : "Pin", icon: "pin", action: { model.setPinned(bot, !bot.pinned) }),
            .init(title: "Mark as Read", icon: "bell.badge", action: { model.markAllRead(bot.id) }),
            .init(title: "Edit Profile…", icon: "square.and.pencil", divider: true,
                  action: { editing = EditTarget(request: EditorRequest(BotDraft(bot)), store: model) }),
            .init(title: "Copy conversation ID", icon: "square.on.square", divider: true, action: {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(bot.id, forType: .string)
            }),
            .init(title: "New session", icon: "arrow.counterclockwise", action: { newSessionBot = target }),
            .init(title: "Hide from sidebar", icon: "eye.slash", divider: true,
                  action: { model.setHidden(bot, true) }),
            .init(title: "Delete", icon: "trash", destructive: true, action: { confirmDelete = target })
        ]
    }
}

/// Usage per computer, read live from the stores.
private struct UsageSheet: View {
    @Environment(HostController.self) private var host

    var body: some View {
        let stores = host.accounts.computers.compactMap { host.accounts.store(for: $0.id) }
        let withUsage = stores.filter { !$0.usage.providers.isEmpty }
        VStack(alignment: .leading, spacing: 0) {
            ModalHeader("Usage")
            VStack(alignment: .leading, spacing: 20) {
                if withUsage.isEmpty {
                    Text("No usage information yet.").foregroundStyle(Palette.secondary)
                } else {
                    ForEach(withUsage, id: \.computer.id) { store in
                        if stores.count > 1 {
                            Label { Text(store.hostName) } icon: { ComputerBadge(store.computer, size: 16) }
                                .appFont(.headline)
                        }
                        UsageLimits(usage: store.usage)
                    }
                }
            }
            .padding([.horizontal, .bottom], 24)
            .padding(.top, 4)
        }
    }
}

extension View {
    @ViewBuilder fileprivate func hiddenWindowTitle() -> some View {
        if #available(macOS 15.0, *) { toolbar(removing: .title) } else { self }
    }
}

/// Window-local floating surface: no NSMenu, NSPopover or system menu styling.
private struct SidebarAccountPanel: View {
    let name: String
    let signedIn: Bool
    let email: String?
    let busy: Bool
    let errorMessage: String?
    let onSignIn: (AccountSession.SignInProvider) -> Void
    let onSignOut: () -> Void
    let compact: Bool
    let approvals: Int
    /// A release Sparkle found, waiting to be installed.
    let updateVersion: String?
    let canUpdate: Bool
    let onDismiss: () -> Void
    let onUsage: () -> Void
    let onComputers: () -> Void
    let onUpdate: () -> Void
    let onToggleSidebar: () -> Void
    let onSearch: () -> Void
    @State private var page = "main"
    @State private var highlighted = 0
    @FocusState private var menuFocused: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private struct Item {
        let title: String
        let icon: String
        var assetIcon: String? = nil
        var detail: String? = nil
        var chevron = false
        var disabled = false
        let action: () -> Void
    }

    private var items: [Item] {
        switch page {
        case "support":
            return [
                Item(title: "Support", icon: "chevron.left", action: { navigate("main") }),
                Item(title: "Help & documentation", icon: "book", chevron: true,
                     action: { open("https://github.com/leepokai/codync#readme") }),
                Item(title: "Report an issue", icon: "bubble.left", chevron: true,
                     action: { open("https://github.com/leepokai/codync/issues") })
            ]
        case "settings":
            return [
                Item(title: "Settings", icon: "chevron.left", action: { navigate("main") }),
                Item(title: compact ? "Expand sidebar" : "Collapse sidebar", icon: "sidebar.left", action: onToggleSidebar),
                Item(title: "Search bots", icon: "magnifyingglass", detail: "⌘F", action: onSearch)
            ]
        default:
            return [
                Item(title: "Usage", icon: "gauge.with.dots.needle.33percent", chevron: true, action: onUsage),
                Item(title: "Computers & devices", icon: "desktopcomputer",
                     detail: approvals > 0 ? "\(approvals)" : nil, chevron: true, action: onComputers),
                Item(title: "Get Codync for mobile", icon: "iphone", action: { open("https://apps.apple.com/app/id6760984418") }),
                Item(title: "Support", icon: "book.closed", chevron: true, action: { navigate("support") }),
                Item(title: "Settings", icon: "gearshape", action: { navigate("settings") }),
                Item(title: updateVersion.map { "Update to \($0)" } ?? "Check for updates", icon: "arrow.down.circle",
                     detail: updateVersion == nil ? Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String : nil,
                     disabled: !canUpdate, action: onUpdate),
                Item(title: compact ? "Expand sidebar" : "Collapse sidebar", icon: "sidebar.left", action: onToggleSidebar)
            ] + authenticationItems
        }
    }

    private var authenticationItems: [Item] {
        if signedIn {
            return [Item(title: busy ? "Please wait…" : "Sign out",
                         icon: "rectangle.portrait.and.arrow.right", disabled: busy, action: onSignOut)]
        }
        return [
            Item(title: "Continue with Apple", icon: "apple.logo", disabled: busy,
                 action: { onSignIn(.apple) }),
            Item(title: "Continue with Google", icon: "person.crop.circle.badge.plus", assetIcon: "google", disabled: busy,
                 action: { onSignIn(.google) })
        ]
    }

    var body: some View {
        menuContent
        .padding(7)
        .background(Color(light: 0xFAFAFA, dark: 0x1B1B1B), in: RoundedRectangle(cornerRadius: 18))
        .shadow(color: .black.opacity(0.25), radius: 20, x: 0, y: 8)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Account menu")
        .focusable()
        .focused($menuFocused)
        .focusEffectDisabled()
        .onKeyPress(.escape) { onDismiss(); return .handled }
        .onKeyPress(.downArrow) { moveFocus(1); return .handled }
        .onKeyPress(.upArrow) { moveFocus(-1); return .handled }
        .onKeyPress(.tab, phases: .down) { press in moveFocus(press.modifiers.contains(.shift) ? -1 : 1); return .handled }
        .onKeyPress(.return) { activate(); return .handled }
        .onKeyPress(.space) { activate(); return .handled }
        .task { await Task.yield(); menuFocused = true }
    }

    private var menuContent: some View {
        VStack(spacing: 2) {
            ForEach(items.indices, id: \.self) { index in
                menuRow(items[index], index: index)
            }
            if let errorMessage, page == "main" {
                Text(errorMessage).appFont(.system(size: 12)).foregroundStyle(Palette.warning)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 10).padding(.vertical, 8)
            }
        }
    }

    @ViewBuilder private func menuRow(_ item: Item, index: Int) -> some View {
        if index == (page == "main" ? 6 : 1) { divider }
        if page == "main" && index == 7 { accountIdentity }
        AccountPanelRow(title: item.title, icon: item.icon, assetIcon: item.assetIcon, detail: item.detail,
                        chevron: item.chevron, keyboardFocused: highlighted == index, action: item.action)
            .disabled(item.disabled)
            .onHover { if $0 { highlighted = index } }
    }

    private var accountIdentity: some View {
        HStack(spacing: 8) {
            Image(systemName: "person.crop.circle").appFont(.system(size: 15))
            Text(email ?? name).appFont(.system(size: 12)).lineLimit(1)
            Spacer()
            if !signedIn { Text("Not signed in").appFont(.system(size: 11)) }
        }
        .foregroundStyle(Palette.secondary)
        .padding(.horizontal, 10)
        .frame(height: 30)
    }

    private var divider: some View {
        Rectangle().fill(Palette.text.opacity(0.13)).frame(height: 0.5)
            .padding(.horizontal, 10).padding(.vertical, 5)
    }

    private func navigate(_ destination: String) {
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
            page = destination
            highlighted = 0
        }
        menuFocused = true
    }

    private func moveFocus(_ delta: Int) {
        highlighted = (highlighted + delta + items.count) % items.count
    }

    private func activate() {
        guard items.indices.contains(highlighted), !items[highlighted].disabled else { return }
        items[highlighted].action()
    }

    private func open(_ string: String) {
        guard let url = URL(string: string) else { return }
        onDismiss()
        NSWorkspace.shared.open(url)
    }
}

private struct AccountPanelRow: View {
    let title: String
    let icon: String
    var assetIcon: String? = nil
    let detail: String?
    let chevron: Bool
    let keyboardFocused: Bool
    var destructive = false
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let assetIcon {
                    Image(assetIcon).resizable().scaledToFit().frame(width: 16, height: 16).frame(width: 20)
                } else {
                    Image(systemName: icon).appFont(.system(size: 14, weight: .regular)).frame(width: 20)
                }
                Text(title).appFont(.system(size: 12)).lineLimit(1)
                Spacer(minLength: 8)
                if let detail { Text(detail).appFont(.system(size: 13)).foregroundStyle(Palette.secondary) }
                if chevron { Image(systemName: "chevron.right").appFont(.system(size: 11)).foregroundStyle(Palette.secondary) }
            }
            .foregroundStyle(destructive ? Palette.danger : Palette.text)
            .padding(.horizontal, 10)
            .frame(height: 30)
            .background(hovered || keyboardFocused ? Palette.text.opacity(0.08) : .clear,
                        in: RoundedRectangle(cornerRadius: 10))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
        .onHover { hovered = $0 }
    }
}

/// A SwiftUI action surface shared by pointer and accessibility entry points.
private struct DesktopActionMenu: View {
    struct Item {
        let title: String
        let icon: String
        var divider = false
        var destructive = false
        let action: () -> Void
    }

    let items: [Item]
    let onDismiss: () -> Void
    @State private var highlighted = 0
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 2) {
            ForEach(items.indices, id: \.self) { index in
                let item = items[index]
                if item.divider {
                    Rectangle().fill(Palette.text.opacity(0.13)).frame(height: 0.5)
                        .padding(.horizontal, 10).padding(.vertical, 5)
                }
                AccountPanelRow(title: item.title, icon: item.icon, detail: nil,
                                chevron: false, keyboardFocused: highlighted == index,
                                destructive: item.destructive) { activate(index) }
                    .onHover { if $0 { highlighted = index } }
            }
        }
        .padding(7)
        .background(Color(light: 0xFAFAFA, dark: 0x1B1B1B), in: RoundedRectangle(cornerRadius: 14))
        .shadow(color: .black.opacity(0.25), radius: 20, x: 0, y: 8)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Conversation actions")
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress(.escape) { onDismiss(); return .handled }
        .onKeyPress(.downArrow) { move(1); return .handled }
        .onKeyPress(.upArrow) { move(-1); return .handled }
        .onKeyPress(.tab, phases: .down) { press in move(press.modifiers.contains(.shift) ? -1 : 1); return .handled }
        .onKeyPress(.return) { activate(highlighted); return .handled }
        .onKeyPress(.space) { activate(highlighted); return .handled }
        .task { await Task.yield(); focused = true }
    }

    private func move(_ delta: Int) { highlighted = (highlighted + delta + items.count) % items.count }

    private func activate(_ index: Int) {
        guard items.indices.contains(index) else { return }
        onDismiss()
        items[index].action()
    }
}
