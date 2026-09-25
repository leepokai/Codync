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
    /// Sheets hang from the window's title bar; sized from it so they never run past its bottom edge.
    @State private var windowSize = CGSize(width: 1100, height: 760)

    private var sheetHeight: CGFloat { max(420, windowSize.height - 76) }

    var body: some View {
        @Bindable var model = model
        NavigationSplitView(columnVisibility: $columns) {
            List(selection: $model.selection) {
                ConnectionBanner()
                if !model.usage.providers.isEmpty {
                    UsageStrip(usage: model.usage)
                        .listRowSeparator(.hidden)
                }
                ForEach(model.roster) { bot in
                    BotRow(bot: bot)
                        .tag(bot.id)
                        .contextMenu {
                            Button(bot.pinned ? "Unpin" : "Pin") { model.setPinned(bot, !bot.pinned) }
                            Button("Edit Profile…") { editing = EditorRequest(BotDraft(bot)) }
                            Button("Mark as Read") { model.markRead(bot.id) }
                            Button("Hide from List") { model.setHidden(bot, true) }
                            Divider()
                            Button("Delete…", role: .destructive) { confirmDelete = bot }
                        }
                }
            }
            .navigationSplitViewColumnWidth(min: 260, ideal: 300, max: 420)
            .navigationTitle(model.hostName)
            .toolbar {
                ToolbarItem {
                    Button("Marketplace", systemImage: "square.grid.2x2") { showPlugins = true }
                        .help("Marketplace: agents, connectors and skills")
                }
                ToolbarItem {
                    Button("New Message", systemImage: "square.and.pencil") {
                        model.selection = nil
                        composing = true
                    }
                        .keyboardShortcut("n")
                        .disabled(model.connection != .online)
                }
            }
        } detail: {
            if composing {
                NewChatView { composing = false }
            } else if let id = model.selection, model.bots[id] != nil {
                NavigationStack { ThreadView(botId: id) }
                    .id(id)
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

private extension View {
    /// The toolbar shows icons only, no "Codync" title next to them.
    @ViewBuilder func hiddenWindowTitle() -> some View {
        if #available(macOS 15.0, *) { toolbar(removing: .title) } else { self }
    }
}
