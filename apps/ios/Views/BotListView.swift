import CodyncKit
import CodyncUI
import SwiftUI

/// The roster: every bot is a person you can message (Grok Bot sidebar, phone-sized).
struct BotListView: View {
    @Environment(BotStore.self) private var model
    @State private var editing: EditorRequest?
    @State private var confirmDelete: Bot?

    var body: some View {
        let roster = model.roster
        ScrollView {
            LazyVStack(spacing: 0) {
                ConnectionBanner()
                    .padding(.vertical, 4)

                if roster.isEmpty {
                    EmptyRoster { editing = EditorRequest(BotDraft()) }
                }

                ForEach(roster) { bot in
                    // A plain button instead of a NavigationLink: same push, no chevron.
                    Button { model.selection = bot.id } label: {
                        BotRow(bot: bot)
                    }
                    .buttonStyle(.plain)
                    .contextActions {
                        [
                            MenuItem(bot.pinned ? "Unpin" : "Pin", icon: bot.pinned ? "pin.slash" : "pin") { model.setPinned(bot, !bot.pinned) },
                            MenuItem("Edit profile", icon: "pencil") { editing = EditorRequest(BotDraft(bot)) },
                            MenuItem("Mark as read", icon: "checkmark.message") { model.markRead(bot.id) },
                            MenuItem("Hide from list", icon: "eye.slash") { model.setHidden(bot, true) },
                            MenuItem("Delete", icon: "trash", destructive: true, divider: true) { confirmDelete = bot },
                        ]
                    }
                }
            }
            .padding(.horizontal, 16)
        }
        .background(Palette.background)
        .navigationTitle(model.hostName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                AccountSwitcherButton()
            }
            // Grok-style bare top bar: no visible title, just the two buttons.
            ToolbarItem(placement: .principal) { Color.clear.frame(width: 1, height: 1) }
            if model.screen != nil {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Screen", systemImage: "display") { model.screenRequest = ScreenRequest() }
                        .disabled(model.connection != .online)
                }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("New bot", systemImage: "plus") {
                    editing = EditorRequest(BotDraft())
                }
                .disabled(model.connection != .online)
            }
        }
        .refreshable { model.restartStream() }
        .sheet(item: $editing) { request in
            NavigationStack { BotEditorView(draft: request.draft) }
        }
        .sheet(isPresented: Bindable(model).showPlugins) {
            NavigationStack {
                MarketplaceView { model.showPlugins = false }
            }
        }
        .sheet(isPresented: Bindable(model).showProfile) {
            NavigationStack { SettingsView() }
        }
        .deleteBotConfirmation($confirmDelete)
    }
}

private struct EmptyRoster: View {
    let create: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            CharacterAvatar(shape: "cloud", color: "green", size: 72)
            Text("No bots yet").font(.title3.weight(.semibold)).foregroundStyle(Palette.text)
            Text("Create a bot for each kind of work — a reviewer, a fixer, a docs writer — and point it at a project.")
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
            Button("Create your first bot", action: create)
                .buttonStyle(.primary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
    }
}

