import CodyncKit
import CodyncUI
import SwiftUI

/// The roster: every bot is a person you can message (Grok Bot sidebar, phone-sized).
struct BotListView: View {
    @Environment(BotStore.self) private var model
    @State private var editing: EditorRequest?
    @State private var showSettings = false
    @State private var confirmDelete: Bot?

    var body: some View {
        let roster = model.roster
        List {
            Section {
                ConnectionBanner()
                if !model.usage.providers.isEmpty {
                    UsageStrip(usage: model.usage)
                }
            }
            .listRowSeparator(.hidden)
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets(top: 4, leading: 16, bottom: 4, trailing: 16))

            if roster.isEmpty {
                EmptyRoster { editing = EditorRequest(BotDraft()) }
                    .listRowSeparator(.hidden)
                    .listRowBackground(Color.clear)
            }

            ForEach(roster) { bot in
                NavigationLink(value: bot.id) {
                    BotRow(bot: bot)
                }
                .listRowBackground(Palette.background)
                .listRowSeparatorTint(Palette.border)
                .swipeActions(edge: .leading) {
                    Button(bot.pinned ? "Unpin" : "Pin", systemImage: bot.pinned ? "pin.slash" : "pin") {
                        model.setPinned(bot, !bot.pinned)
                    }
                    .tint(Palette.accentDim)
                }
                .swipeActions(edge: .trailing) {
                    Button("Delete", systemImage: "trash", role: .destructive) { confirmDelete = bot }
                    Button("Hide", systemImage: "eye.slash") { model.setHidden(bot, true) }
                }
                .contextMenu {
                    Button(bot.pinned ? "Unpin" : "Pin", systemImage: "pin") { model.setPinned(bot, !bot.pinned) }
                    Button("Edit profile", systemImage: "pencil") { editing = EditorRequest(BotDraft(bot)) }
                    Button("Mark as read", systemImage: "checkmark.message") { model.markRead(bot.id) }
                    Button("Hide from list", systemImage: "eye.slash") { model.setHidden(bot, true) }
                    Button("Delete", systemImage: "trash", role: .destructive) { confirmDelete = bot }
                }
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Palette.background)
        .navigationTitle(model.hostName)
        .navigationBarTitleDisplayMode(.large)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Button("Settings", systemImage: "gearshape") { showSettings = true }
            }
            ToolbarItem(placement: .topBarTrailing) {
                Button("New bot", systemImage: "plus") {
                    editing = EditorRequest(BotDraft())
                }
                .disabled(model.connection != .online)
            }
        }
        .refreshable {
            model.restartStream()
            await model.refreshUsage()
        }
        .sheet(item: $editing) { request in
            NavigationStack { BotEditorView(draft: request.draft) }
        }
        .sheet(isPresented: $showSettings) {
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
                .buttonStyle(.borderedProminent)
                .tint(Palette.accentFill)
                .foregroundStyle(Palette.onAccent)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
    }
}

