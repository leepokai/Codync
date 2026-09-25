import CodyncKit
import CodyncUI
import SwiftUI

/// Native desktop chat: roster on the left, the conversation on the right
/// (Grok Bot's desktop layout), sharing its views with the iPhone app.
struct ChatWindow: View {
    @Environment(HostController.self) private var host

    var body: some View {
        Group {
            if host.state == .running || !host.accounts.computers.isEmpty {
                ChatSplitView().id(host.contextID)
            } else {
                ContentUnavailableView(
                    "Codync host isn't running",
                    systemImage: "desktopcomputer.trianglebadge.exclamationmark",
                    description: Text("Open Codync from the menu bar to install or restart it.")
                )
            }
        }
        .frame(minWidth: 760, minHeight: 500)
        .background(Palette.background)
        .tint(Palette.accent)
        .ignoresSafeArea(.container, edges: .top)
        // Approving a device: the code the device shows must match (spec §4.2 B).
        // It closes when the request is decided or put off (both change `currentApproval`).
        .sheet(item: Binding(get: { host.currentApproval }, set: { _ in })) {
            ApprovalSheet(approval: $0)
        }
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
    @State private var editing: EditTarget?
    @State private var confirmDelete: BotTarget?
    @State private var contextBot: BotTarget?
    @State private var contextPoint = CGPoint.zero
    @State private var newSessionBot: BotTarget?
    @State private var composing = false
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
    @AppStorage("sidebarCompact") private var compact = false
    @AppStorage("desktopSidebarWidth") private var sidebarWidth = 296.0
    @State private var dragStartWidth: Double?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Sheets hang from the window's title bar; sized from it so they never run past its bottom edge.
    @State private var windowSize = CGSize(width: 1100, height: 760)

    private var sheetHeight: CGFloat { max(420, windowSize.height - 76) }

    private var accounts: AccountStore { host.accounts }
    /// Every computer with a store, in the account's order.
    private var stores: [BotStore] { accounts.computers.compactMap { accounts.store(for: $0.id) } }
    private var onlineStores: [BotStore] { stores.filter { $0.connection == .online } }
    private var selectedStore: BotStore? { accounts.selection.flatMap { accounts.store(for: $0.computerId) } }
    private var composeStore: BotStore? { composeComputer.flatMap { accounts.store(for: $0) } }
    private var showsComputers: Bool { stores.count > 1 && !compact }

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
                HStack {
                    Spacer()
                    Button("New chat", systemImage: "plus", action: compose)
                        .labelStyle(.iconOnly)
                        .buttonStyle(.plain)
                        .help("New chat (⌘N)")
                        .keyboardShortcut("n")
                        .disabled(onlineStores.isEmpty)
                }
                .padding(.leading, compact ? 0 : 80)
                .padding(.trailing, 18)
                .frame(height: 44)
                .opacity(compact ? 0 : 1)
                .allowsHitTesting(!compact)
                .accessibilityHidden(compact)
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(stores, id: \.computer.id) { store in
                            let bots = filteredBots(store)
                            if showsComputers && !(bots.isEmpty && !search.isEmpty) {
                                ComputerHeader(store: store, ssh: host.isSSH(store.computer.id))
                            }
                            ForEach(bots) { bot in row(bot, store) }
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.top, 4)
                }
                .onMoveCommand { direction in
                    let refs = stores.flatMap { store in filteredBots(store).map { ref($0, store) } }
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
                            ForEach(stores, id: \.computer.id) { store in
                                ConnectionBanner().environment(store).padding(.horizontal, 12)
                            }
                        }
                        .frame(width: sidebarWidth)
                        .transition(.opacity)
                    }
                }
                .overlay {
                    if stores.allSatisfy({ filteredBots($0).isEmpty }) && !compact {
                        VStack(spacing: 8) {
                            Text(search.isEmpty ? "No bots yet" : "No matching bots")
                                .font(.system(size: 13, weight: .medium))
                            Text(search.isEmpty ? "Use + to start a new chat." : "Try another name or message.")
                                .font(.system(size: 12)).foregroundStyle(Palette.secondary)
                        }
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
                        NewChatView {
                            composing = false
                            if accounts.selection == nil { accounts.selection = previousSelection }
                        }
                        .environment(store)
                        .id(store.computer.id)
                    }
                    .ignoresSafeArea(.container, edges: .top)

                } else if let ref = accounts.selection, let store = selectedStore, store.bots[ref.botId] != nil {
                    ThreadView(botId: ref.botId)
                        .environment(store)
                        .id(ref)
                        .transition(.asymmetric(insertion: .opacity, removal: .identity))
                } else {
                    VStack(spacing: 14) {
                        HStack(spacing: -10) {
                            CharacterAvatar(shape: "blob", color: "blue", size: 60, mood: .working)
                            CharacterAvatar(shape: "squircle", color: "orange", size: 60)
                            CharacterAvatar(shape: "teardrop", color: "violet", size: 60, mood: .working)
                        }
                        Text("Your coding agents, as teammates.").font(.title2.weight(.semibold))
                        Text("Pick a bot, or create one for each kind of work and point it at a project.")
                            .foregroundStyle(Palette.secondary)
                            .multilineTextAlignment(.center)
                            .frame(maxWidth: 420)
                        Button("New Bot", action: compose)
                            .buttonStyle(.plain)
                            .font(.body.weight(.semibold))
                            .padding(.horizontal, 18)
                            .padding(.vertical, 9)
                            .background(Palette.accentFill, in: Capsule())
                            .foregroundStyle(Palette.onAccent)
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
                        errorMessage: account.errorMessage,
                        onAuthenticate: {
                            dismissAccountMenu()
                            Task {
                                if account.isSignedIn { await host.signOut() }
                                else { await account.signIn() }
                                if account.errorMessage != nil { showAccount = true }
                            }
                        },
                        compact: compact,
                        usage: stores.flatMap(\.usage.providers).flatMap(\.windows).map(\.percent).max(),
                        approvals: host.approvals.count,
                        onDismiss: dismissAccountMenu,
                        onUsage: { dismissAccountMenu(); showUsage = true },
                        onComputers: { dismissAccountMenu(); showComputers = true },
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
        .ignoresSafeArea(.container, edges: .top)
        .sheet(item: $editing) { target in
            NavigationStack { BotEditorView(draft: target.request.draft) }
                .environment(target.store)
                .frame(width: 520, height: min(680, sheetHeight))
        }
        .sheet(isPresented: Binding(get: { marketplace != nil }, set: { if !$0 { marketplace = nil } })) {
            if let store = marketplace.flatMap(accounts.store(for:)) {
                NavigationStack {
                    MarketplaceView { marketplace = nil }
                }
                .environment(store)
                .frame(width: min(920, windowSize.width - 80), height: sheetHeight)
            }
        }
        .sheet(isPresented: $showComputers) {
            ComputersView { showComputers = false }
                .frame(width: 620, height: sheetHeight)
        }
        .sheet(isPresented: $showUsage) {
            VStack(alignment: .leading, spacing: 20) {
                HStack {
                    Text("Usage").font(.title2.bold())
                    Spacer()
                    Button("Done") { showUsage = false }.keyboardShortcut(.cancelAction)
                }
                let withUsage = stores.filter { !$0.usage.providers.isEmpty }
                if withUsage.isEmpty {
                    Text("No usage information yet.").foregroundStyle(Palette.secondary)
                } else {
                    ForEach(withUsage, id: \.computer.id) { store in
                        if stores.count > 1 {
                            Label { Text(store.hostName) } icon: { ComputerBadge(store.computer, size: 16) }
                                .font(.headline)
                        }
                        UsageStrip(usage: store.usage)
                    }
                }
            }
            .padding(24)
            .frame(width: 520)
        }
        .background {
            collapseButton.hidden()
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
        .codyncDialog("Start a new session?", isPresented: Binding(
            get: { newSessionBot != nil },
            set: { if !$0 { newSessionBot = nil } }
        ), message: "The conversation stays here, but the agent starts with a fresh context.") {
            guard let target = newSessionBot else { return [] }
            return [DialogAction("New session") { target.store.newSession(target.bot.id); newSessionBot = nil }]
        }
        .confirmationDialog("Delete \(confirmDelete?.bot.name ?? "bot")?", isPresented: Binding(
            get: { confirmDelete != nil },
            set: { if !$0 { confirmDelete = nil } }
        ), titleVisibility: .visible) {
            if let target = confirmDelete {
                Button("Delete bot and its conversation", role: .destructive) { target.store.delete(target.bot) }
            }
        } message: {
            Text("Files it changed on your computer stay as they are.")
        }
        .alert("Something went wrong", isPresented: Binding(get: { errorMessage != nil }, set: { if !$0 { clearErrors() } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(errorMessage ?? "")
        }
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
                // Screenshot/UI checks: CODYNC_DEBUG_OPEN=compose | plugins | computers | <bot name>
                let target = ProcessInfo.processInfo.environment["CODYNC_DEBUG_OPEN"]
                if target == "compose" {
                    compose()
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

    fileprivate func filteredBots(_ store: BotStore) -> [Bot] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        if compact { return store.roster }
        return store.roster.filter {
            query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
                || ($0.lastMessage?.localizedCaseInsensitiveContains(query) ?? false)
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
            }
        }
        .font(.system(size: 13))
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
            Picker("Computer", selection: Binding(get: { composeComputer ?? "" }, set: { composeComputer = $0 })) {
                ForEach(onlineStores, id: \.computer.id) { store in
                    Text(store.hostName).tag(store.computer.id)
                }
            }
            .labelsHidden()
            .pickerStyle(.menu)
            .fixedSize()
            Spacer()
        }
        .font(.callout)
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
            .font(.system(size: 15, weight: .medium))
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
                        .font(.system(size: 14, weight: .regular))
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
        .font(.system(size: 13, weight: .medium))
        .foregroundStyle(Palette.text)
        .padding(.horizontal, 12)
        .padding(.top, 6)
        .padding(.bottom, 10)
        .background(Palette.surface)
    }

    fileprivate func compose() {
        // The selected bot's computer if it's online, else this Mac, else any online one.
        let target = [selectedStore, host.store].compactMap { $0 }.first { $0.connection == .online } ?? onlineStores.first
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
            Button("New chat", systemImage: "plus", action: compose)
                .disabled(onlineStores.isEmpty)
                .help("New chat")
                .labelStyle(.iconOnly)
                .buttonStyle(IconButtonStyle(size: 36))
            Button("Marketplace", systemImage: "square.grid.2x2") {
                marketplace = (selectedStore ?? host.store ?? onlineStores.first)?.computer.id
            }
                .help("Marketplace")
                .labelStyle(.iconOnly)
                .buttonStyle(IconButtonStyle(size: 36))
            Button { showAccount.toggle() } label: {
                profileAvatar
                    .overlay(alignment: .bottomTrailing) {
                        Circle().fill(host.store?.connection == .online ? Palette.switchOn : Palette.tertiary)
                            .frame(width: 7, height: 7)
                            .overlay(Circle().stroke(Palette.surface, lineWidth: 1.5))
                    }
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
            .init(title: "Mark as Read", icon: "bell.badge", action: { model.markRead(bot.id) }),
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
    let onAuthenticate: () -> Void
    let compact: Bool
    let usage: Double?
    let approvals: Int
    let onDismiss: () -> Void
    let onUsage: () -> Void
    let onComputers: () -> Void
    let onToggleSidebar: () -> Void
    let onSearch: () -> Void
    @State private var page = "main"
    @State private var highlighted = 0
    @FocusState private var menuFocused: Bool

    private struct Item {
        let title: String
        let icon: String
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
                Item(title: "Usage", icon: "gauge.with.dots.needle.33percent",
                     detail: usage.map { "\(Int($0.rounded()))%" }, chevron: true, action: onUsage),
                Item(title: "Computers & devices", icon: "desktopcomputer",
                     detail: approvals > 0 ? "\(approvals)" : nil, chevron: true, action: onComputers),
                Item(title: "Get Codync for mobile", icon: "iphone", action: { open("https://apps.apple.com/app/id6760984418") }),
                Item(title: "Support", icon: "book.closed", chevron: true, action: { navigate("support") }),
                Item(title: "Settings", icon: "gearshape", action: { navigate("settings") }),
                Item(title: compact ? "Expand sidebar" : "Collapse sidebar", icon: "sidebar.left", action: onToggleSidebar),
                Item(title: busy ? "Please wait…" : signedIn ? "Log out" : "Continue with Google",
                     icon: signedIn ? "rectangle.portrait.and.arrow.right" : "person.crop.circle.badge.plus",
                     disabled: busy, action: onAuthenticate)
            ]
        }
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
                Text(errorMessage).font(.system(size: 12)).foregroundStyle(Palette.warning)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.horizontal, 10).padding(.vertical, 8)
            }
        }
    }

    @ViewBuilder private func menuRow(_ item: Item, index: Int) -> some View {
        if index == (page == "main" ? 5 : 1) { divider }
        if page == "main" && index == 6 { accountIdentity }
        AccountPanelRow(title: item.title, icon: item.icon, detail: item.detail,
                        chevron: item.chevron, keyboardFocused: highlighted == index, action: item.action)
            .disabled(item.disabled)
            .onHover { if $0 { highlighted = index } }
    }

    private var accountIdentity: some View {
        HStack(spacing: 8) {
            Image(systemName: "person.crop.circle").font(.system(size: 15))
            Text(email ?? name).font(.system(size: 12)).lineLimit(1)
            Spacer()
            if !signedIn { Text("Not signed in").font(.system(size: 11)) }
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
        page = destination
        highlighted = 0
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
    let detail: String?
    let chevron: Bool
    let keyboardFocused: Bool
    var destructive = false
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: icon).font(.system(size: 14, weight: .regular)).frame(width: 20)
                Text(title).font(.system(size: 12)).lineLimit(1)
                Spacer(minLength: 8)
                if let detail { Text(detail).font(.system(size: 13)).foregroundStyle(Palette.secondary) }
                if chevron { Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Palette.secondary) }
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
