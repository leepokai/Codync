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

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
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
                    Button("New Bot", systemImage: "square.and.pencil") { editing = EditorRequest(BotDraft()) }
                        .keyboardShortcut("n")
                        .disabled(model.connection != .online)
                }
            }
        } detail: {
            if let id = model.selection, model.bots[id] != nil {
                NavigationStack { ThreadView(botId: id) }
                    .id(id)
            } else {
                VStack(spacing: 14) {
                    HStack(spacing: -10) {
                        CharacterAvatar(shape: "blob", color: "blue", size: 60, mood: .working)
                        CharacterAvatar(shape: "squircle", color: "orange", size: 60)
                        CharacterAvatar(shape: "teardrop", color: "violet", size: 60, mood: .working)
                    }
                    Text("Your coding agents, as teammates.").font(.title2.bold())
                    Text("Pick a bot, or create one for each kind of work and point it at a project.")
                        .foregroundStyle(Palette.secondary)
                    Button("New Bot") { editing = EditorRequest(BotDraft()) }
                        .buttonStyle(.borderedProminent)
                        .tint(Palette.accentFill)
                        .foregroundStyle(Palette.onAccent)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .sheet(item: $editing) { request in
            NavigationStack { BotEditorView(draft: request.draft) }
                .frame(minWidth: 520, minHeight: 640)
        }
        .deleteBotConfirmation($confirmDelete)
        .storeErrorAlert(model)
    }
}
