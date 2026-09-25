import CodyncKit
import CodyncUI
import SwiftUI

/// Native desktop chat: roster on the left, the conversation on the right
/// (Grok Bot's desktop layout), sharing its views with the iPhone app.
struct ChatWindow: View {
    @Environment(HostController.self) private var host

    var body: some View {
        Group {
            if let store = host.store {
                ChatSplitView().environment(store)
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
    }
}

private struct ChatSplitView: View {
    @Environment(BotStore.self) private var model
    @Environment(AccountSession.self) private var account
    @State private var editing: EditorRequest?
    @State private var confirmDelete: Bot?
    @State private var composing = false
    @State private var previousSelection: String?
    @State private var hoveredBot: String?
    @State private var showPlugins = false
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
                        .disabled(model.connection != .online)
                }
                .padding(.leading, compact ? 0 : 80)
                .padding(.trailing, 18)
                .frame(height: 44)
                .opacity(compact ? 0 : 1)
                .allowsHitTesting(!compact)
                .accessibilityHidden(compact)
                ScrollView {
                    LazyVStack(spacing: 2) {
                        ForEach(filteredBots) { bot in
                            Button {
                                model.selection = bot.id
                            } label: {
                                BotRow(bot: bot, compact: compact)
                                    .padding(.horizontal, compact ? 0 : 8)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(
                                    model.selection == bot.id
                                        ? Palette.bubbleUser : hoveredBot == bot.id ? Palette.bubbleAgent : Color.clear,
                                    in: RoundedRectangle(cornerRadius: 10)
                                )
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel(bot.name)
                            .accessibilityAddTraits(model.selection == bot.id ? .isSelected : [])
                            .help(bot.name)
                            .onHover { hoveredBot = $0 ? bot.id : nil }
                            .contextMenu { botMenu(bot) }
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.top, 4)
                }
                .onMoveCommand { direction in
                    guard direction == .up || direction == .down, !filteredBots.isEmpty else { return }
                    let current = filteredBots.firstIndex { $0.id == model.selection } ?? -1
                    let next = direction == .down ? min(current + 1, filteredBots.count - 1) : max(current - 1, 0)
                    model.selection = filteredBots[next].id
                }
                .scrollContentBackground(.hidden)
                .background(Palette.surface)
                .safeAreaInset(edge: .top, spacing: 0) {
                    if !compact {
                        VStack(spacing: 0) {
                            searchField
                            ConnectionBanner().padding(.horizontal, 12)
                        }
                        .frame(width: sidebarWidth)
                        .transition(.opacity)
                    }
                }
                .overlay {
                    if filteredBots.isEmpty && !compact {
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
                if composing {
                    // The To: row takes the title bar's place instead of sitting under an empty one.
                    NewChatView {
                        composing = false
                        if model.selection == nil { model.selection = previousSelection }
                    }
                    .ignoresSafeArea(.container, edges: .top)

                } else if let id = model.selection, model.bots[id] != nil {
                    ThreadView(botId: id)
                        .id(id)
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
        .allowsHitTesting(!showAccount)
        .accessibilityHidden(showAccount)
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
                                if account.isSignedIn { await account.signOut() }
                                else { await account.signIn() }
                                if account.errorMessage != nil { showAccount = true }
                            }
                        },
                        compact: compact,
                        usage: model.usage.providers.flatMap(\.windows).map(\.percent).max(),
                        onDismiss: dismissAccountMenu,
                        onUsage: { dismissAccountMenu(); showUsage = true },
                        onToggleSidebar: {
                            dismissAccountMenu()
                            compact.toggle()
                        },
                        onSearch: { dismissAccountMenu(); compact = false; searchFocused = true }
                    )
                    .frame(width: min(288, windowSize.width - 32))
                    .padding(.leading, 16)
                    .padding(.bottom, compact ? 68 : 64)
                    .transition(.scale(scale: 0.97, anchor: .bottomLeading).combined(with: .opacity))
                }
            }
        }
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: showAccount)
        .ignoresSafeArea(.container, edges: .top)
        .sheet(item: $editing) { request in
            NavigationStack { BotEditorView(draft: request.draft) }
                .frame(width: 520, height: min(680, sheetHeight))
        }
        .sheet(isPresented: $showPlugins) {
            NavigationStack {
                MarketplaceView { showPlugins = false }
            }
            .frame(width: min(920, windowSize.width - 80), height: sheetHeight)
        }
        .sheet(isPresented: $showUsage) {
            VStack(alignment: .leading, spacing: 20) {
                HStack {
                    Text("Usage").font(.title2.bold())
                    Spacer()
                    Button("Done") { showUsage = false }.keyboardShortcut(.cancelAction)
                }
                if model.usage.providers.isEmpty {
                    Text("No usage information yet.").foregroundStyle(Palette.secondary)
                } else {
                    UsageStrip(usage: model.usage)
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
        .deleteBotConfirmation($confirmDelete)
        .storeErrorAlert(model)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: model.selection)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: composing)
        .onChange(of: model.selection) { _, id in if id != nil { composing = false } }
        #if DEBUG
            .onAppear {
                // Screenshot/UI checks: CODYNC_DEBUG_OPEN=compose | plugins | <bot name>
                let target = ProcessInfo.processInfo.environment["CODYNC_DEBUG_OPEN"]
                if target == "compose" {
                    composing = true
                } else if target == "plugins" {
                    showPlugins = true
                } else if let target, let bot = model.roster.first(where: { $0.name == target }) {
                    model.selection = bot.id
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

    fileprivate var filteredBots: [Bot] {
        let query = search.trimmingCharacters(in: .whitespacesAndNewlines)
        if compact { return model.roster }
        return model.roster.filter {
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
        .frame(height: 32)
        .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).stroke(Palette.border, lineWidth: 0.5))
        .padding(.horizontal, 12)
        .padding(.top, 4)
        .padding(.bottom, 8)
        .background(Palette.surface)
    }

    private var localProfileName: String { "Account" }

    private var profileAvatar: some View {
        Group {
            if let url = account.avatarURL {
                AsyncImage(url: url) { image in image.resizable().scaledToFill() } placeholder: { profilePlaceholder }
            } else { profilePlaceholder }
        }
        .frame(width: 32, height: 32)
        .background(Palette.bubbleUser, in: Circle())
        .clipShape(Circle())
        .overlay(Circle().strokeBorder(Palette.text.opacity(0.08), lineWidth: 0.5))
        .overlay { if account.isBusy { ProgressView().controlSize(.small) } }
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
                showPlugins = true
            } label: {
                HStack(spacing: 10) {
                    Image(systemName: "square.grid.2x2")
                        .font(.system(size: 14, weight: .regular))
                        .frame(width: 30, height: 30)
                        .background(Palette.text.opacity(0.025), in: Circle())
                        .overlay(Circle().strokeBorder(Palette.text.opacity(0.1), lineWidth: 0.5))
                    Text("Marketplace")
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 8)
                .frame(height: 42)
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
                .frame(height: 44)
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
        if !composing { previousSelection = model.selection }
        model.selection = nil
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
                .disabled(model.connection != .online)
                .help("New chat")
                .labelStyle(.iconOnly)
                .buttonStyle(IconButtonStyle(size: 36))
            Button("Marketplace", systemImage: "square.grid.2x2") { showPlugins = true }
                .help("Marketplace")
                .labelStyle(.iconOnly)
                .buttonStyle(IconButtonStyle(size: 36))
            Button { showAccount.toggle() } label: {
                profileAvatar
                    .overlay(alignment: .bottomTrailing) {
                        Circle().fill(model.connection == .online ? Palette.switchOn : Palette.tertiary)
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

    @ViewBuilder fileprivate func botMenu(_ bot: Bot) -> some View {
        Button(bot.pinned ? "Unpin" : "Pin") { model.setPinned(bot, !bot.pinned) }
        Button("Edit Profile…") { editing = EditorRequest(BotDraft(bot)) }
        Button("Mark as Read") { model.markRead(bot.id) }
        Button("Hide from List") { model.setHidden(bot, true) }
        Divider()
        Button("Delete…", role: .destructive) { confirmDelete = bot }
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
    let onDismiss: () -> Void
    let onUsage: () -> Void
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
        .overlay(RoundedRectangle(cornerRadius: 18).strokeBorder(Palette.text.opacity(0.15), lineWidth: 0.5))
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
                    .padding(.horizontal, 12).padding(.vertical, 8)
            }
        }
    }

    @ViewBuilder private func menuRow(_ item: Item, index: Int) -> some View {
        if index == (page == "main" ? 4 : 1) { divider }
        if page == "main" && index == 5 { accountIdentity }
        AccountPanelRow(title: item.title, icon: item.icon, detail: item.detail,
                        chevron: item.chevron, keyboardFocused: highlighted == index, action: item.action)
            .disabled(item.disabled)
            .onHover { if $0 { highlighted = index } }
    }

    private var accountIdentity: some View {
        HStack(spacing: 10) {
            Image(systemName: "person.crop.circle").font(.system(size: 18))
            Text(email ?? name).font(.system(size: 12)).lineLimit(1)
            Spacer()
            if !signedIn { Text("Not signed in").font(.system(size: 11)) }
        }
        .foregroundStyle(Palette.secondary)
        .padding(.horizontal, 12)
        .frame(height: 38)
    }

    private var divider: some View {
        Rectangle().fill(Palette.text.opacity(0.13)).frame(height: 0.5)
            .padding(.horizontal, 10).padding(.vertical, 7)
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
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: icon).font(.system(size: 16, weight: .regular)).frame(width: 20)
                Text(title).font(.system(size: 14)).lineLimit(1)
                Spacer(minLength: 8)
                if let detail { Text(detail).font(.system(size: 13)).foregroundStyle(Palette.secondary) }
                if chevron { Image(systemName: "chevron.right").font(.system(size: 11)).foregroundStyle(Palette.secondary) }
            }
            .foregroundStyle(Palette.text)
            .padding(.horizontal, 12)
            .frame(height: 40)
            .background(hovered || keyboardFocused ? Palette.text.opacity(0.08) : .clear,
                        in: RoundedRectangle(cornerRadius: 10))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .focusEffectDisabled()
        .onHover { hovered = $0 }
    }
}
