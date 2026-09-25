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
    }
}

private struct ChatSplitView: View {
    @Environment(BotStore.self) private var model
    @State private var editing: EditorRequest?
    @State private var confirmDelete: Bot?
    @State private var composing = false
    @State private var columns: NavigationSplitViewVisibility = .all
    @State private var showPlugins = false
    @AppStorage("sidebarCompact") private var compact = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// Sheets hang from the window's title bar; sized from it so they never run past its bottom edge.
    @State private var windowSize = CGSize(width: 1100, height: 760)

    private var sheetHeight: CGFloat { max(420, windowSize.height - 76) }

    var body: some View {
        @Bindable var model = model
        NavigationSplitView(columnVisibility: $columns) {
            List(selection: $model.selection) {
                if compact {
                    ForEach(model.roster) { bot in
                        AvatarWithStatus(bot: bot, size: 40)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 4)
                            .help(bot.name)
                            .accessibilityLabel(bot.name)
                            .tag(bot.id)
                            .contextMenu { botMenu(bot) }
                    }
                } else {
                    ConnectionBanner()
                    if !model.usage.providers.isEmpty {
                        UsageStrip(usage: model.usage)
                            .listRowSeparator(.hidden)
                    }
                    ForEach(model.roster) { bot in
                        BotRow(bot: bot)
                            .tag(bot.id)
                            .contextMenu { botMenu(bot) }
                    }
                }
            }
            .safeAreaInset(edge: .bottom) {
                if compact { railActions }
            }
            .navigationSplitViewColumnWidth(min: 260, ideal: 300, max: 420)
            .background(SidebarWidth(compact: compact, animated: !reduceMotion))
            .navigationTitle(model.hostName)
            .toolbar {
                if !compact {
                    ToolbarItem {
                        Button("Marketplace", systemImage: "square.grid.2x2") { showPlugins = true }
                            .help("Marketplace: agents, connectors and skills")
                    }
                    ToolbarItem {
                        Button("New Message", systemImage: "square.and.pencil", action: compose)
                            .keyboardShortcut("n")
                            .disabled(model.connection != .online)
                    }
                    ToolbarItem { collapseButton }
                }
            }
            .toolbar(removing: .sidebarToggle)
        } detail: {
            if composing {
                // The To: row takes the title bar's place instead of sitting under an empty one.
                NewChatView { composing = false }
                    .ignoresSafeArea(.container, edges: .top)
                    .toolbarBackground(.hidden, for: .windowToolbar)
            } else if let id = model.selection, model.bots[id] != nil {
                NavigationStack { ThreadView(botId: id) }
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
                    Button("New Bot") { composing = true }
                        .buttonStyle(.plain)
                        .font(.body.weight(.semibold))
                        .padding(.horizontal, 18)
                        .padding(.vertical, 9)
                        .background(Palette.accentFill, in: Capsule())
                        .foregroundStyle(Palette.onAccent)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
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
        .onGeometryChange(for: CGSize.self) { $0.size } action: { windowSize = $0 }
        .hiddenWindowTitle()
        .deleteBotConfirmation($confirmDelete)
        .storeErrorAlert(model)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: model.selection)
        .animation(Motion.reduced(Motion.fade, reduceMotion), value: composing)
        .onChange(of: model.selection) { _, id in if id != nil { composing = false } }
        // Dragging the divider shut lands on the rail, not on nothing.
        .onChange(of: columns) { _, visibility in
            if visibility == .detailOnly {
                compact = true
                columns = .all
            }
        }
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

private extension ChatSplitView {
    func compose() {
        model.selection = nil
        composing = true
    }

    var collapseButton: some View {
        Button(compact ? "Expand Sidebar" : "Collapse Sidebar", systemImage: "sidebar.left") {
            withAnimation(Motion.reduced(Motion.morph, reduceMotion)) { compact.toggle() }
        }
        .keyboardShortcut("s", modifiers: [.control, .command])
        .help(compact ? "Expand sidebar" : "Collapse sidebar")
    }

    /// Rail footer: what the toolbar holds when the sidebar is wide.
    var railActions: some View {
        VStack(spacing: 6) {
            Button("New Message", systemImage: "plus", action: compose)
                .disabled(model.connection != .online)
                .help("New message")
            Button("Marketplace", systemImage: "square.grid.2x2") { showPlugins = true }
                .help("Marketplace: agents, connectors and skills")
            collapseButton
        }
        .labelStyle(.iconOnly)
        .buttonStyle(IconButtonStyle(size: 34))
        .frame(maxWidth: .infinity)
        .padding(.vertical, 12)
    }

    @ViewBuilder func botMenu(_ bot: Bot) -> some View {
        Button(bot.pinned ? "Unpin" : "Pin") { model.setPinned(bot, !bot.pinned) }
        Button("Edit Profile…") { editing = EditorRequest(BotDraft(bot)) }
        Button("Mark as Read") { model.markRead(bot.id) }
        Button("Hide from List") { model.setHidden(bot, true) }
        Divider()
        Button("Delete…", role: .destructive) { confirmDelete = bot }
    }
}

/// Collapsed = an avatar rail (Grok Bot), never a hidden sidebar. SwiftUI applies
/// `navigationSplitViewColumnWidth` only once, so the rail resizes the AppKit split item directly.
private struct SidebarWidth: NSViewRepresentable {
    let compact: Bool
    let animated: Bool

    final class Coordinator {
        var applied = false
        var glide: Task<Void, Never>?
    }

    func makeCoordinator() -> Coordinator { Coordinator() }

    func makeNSView(context: Context) -> NSView { NSView() }

    func updateNSView(_ view: NSView, context: Context) {
        let compact = compact
        // Only on a toggle, so a width the user dragged to stays put.
        guard compact != context.coordinator.applied else { return }
        context.coordinator.applied = compact
        // The view joins the window's split view only after the first layout pass.
        DispatchQueue.main.async {
            var ancestor = view.superview
            while let current = ancestor, !(current is NSSplitView) { ancestor = current.superview }
            guard let split = ancestor as? NSSplitView,
                  let controller = split.delegate as? NSSplitViewController,
                  let item = controller.splitViewItems.first else { return }
            let width: CGFloat = compact ? 76 : 300
            item.canCollapse = false
            let from = item.viewController.view.frame.width
            // Let the divider travel the whole way; the final limits apply once it lands.
            item.minimumThickness = min(from, width, compact ? 76 : 260)
            item.maximumThickness = max(from, width, compact ? 76 : 420)
            let land = {
                item.minimumThickness = compact ? 76 : 260
                item.maximumThickness = compact ? 76 : 420
                split.setPosition(width, ofDividerAt: 0)
            }
            context.coordinator.glide?.cancel()
            guard animated, abs(from - width) > 1 else { return land() }
            context.coordinator.glide = Task { @MainActor in
                let start = ContinuousClock.now
                let duration = Duration.milliseconds(260)  // Grok's rail cluster: width .26s cubic-bezier(.22,1,.36,1)
                var t = 0.0
                while t < 1 {
                    try? await Task.sleep(for: .milliseconds(8))
                    if Task.isCancelled { return }
                    t = min(1, (ContinuousClock.now - start) / duration)
                    split.setPosition(from + (width - from) * Motion.morphCurve.value(at: t), ofDividerAt: 0)
                }
                land()
            }
        }
    }
}

private extension View {
    /// The toolbar shows icons only, no "Codync" title next to them.
    @ViewBuilder func hiddenWindowTitle() -> some View {
        if #available(macOS 15.0, *) { toolbar(removing: .title) } else { self }
    }
}
