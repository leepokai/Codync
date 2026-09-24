import CodyncKit
import SwiftUI

/// The roster: every bot is a person you can message (Grok Bot sidebar, phone-sized).
struct BotListView: View {
    @Environment(AppModel.self) private var model
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
        .confirmationDialog(
            "Delete \(confirmDelete?.name ?? "")?",
            isPresented: Binding(get: { confirmDelete != nil }, set: { if !$0 { confirmDelete = nil } }),
            titleVisibility: .visible
        ) {
            Button("Delete bot and its conversation", role: .destructive) {
                if let bot = confirmDelete { model.delete(bot) }
            }
        } message: {
            Text("Files it changed on your computer stay as they are.")
        }
    }
}

struct BotRow: View {
    let bot: Bot

    var body: some View {
        HStack(spacing: 12) {
            AvatarWithStatus(bot: bot, size: 46)
            VStack(alignment: .leading, spacing: 3) {
                HStack(alignment: .firstTextBaseline) {
                    if bot.pinned {
                        Image(systemName: "pin.fill").font(.caption2).foregroundStyle(Palette.tertiary)
                    }
                    Text(bot.name)
                        .font(.body.weight(bot.unread > 0 ? .semibold : .medium))
                        .foregroundStyle(Palette.text)
                        .lineLimit(1)
                    Spacer(minLength: 8)
                    Text(RelativeTime.short(Date(milliseconds: bot.lastAt)))
                        .font(.caption)
                        .foregroundStyle(bot.unread > 0 ? Palette.accent : Palette.tertiary)
                        .monospacedDigit()
                }
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    preview
                    Spacer(minLength: 4)
                    if bot.unread > 0 {
                        Text("\(bot.unread)")
                            .font(.caption2.bold())
                            .foregroundStyle(Palette.onAccent)
                            .padding(.horizontal, 6)
                            .frame(minWidth: 18, minHeight: 18)
                            .background(Palette.accentFill, in: Capsule())
                    }
                }
            }
        }
        .padding(.vertical, 6)
        .accessibilityElement(children: .combine)
    }

    /// Live activity while working, otherwise the last message (Grok Bot row behavior).
    @ViewBuilder private var preview: some View {
        if bot.needsInput {
            Label(bot.activity.isEmpty ? "Needs your approval" : bot.activity, systemImage: "hand.raised.fill")
                .font(.subheadline)
                .foregroundStyle(Palette.warning)
                .lineLimit(1)
        } else if bot.isWorking {
            HStack(spacing: 6) {
                ProgressView().controlSize(.mini)
                Text(bot.activity.isEmpty ? "Working…" : bot.activity)
                    .lineLimit(1)
            }
            .font(.subheadline)
            .foregroundStyle(Palette.accent)
        } else if bot.status == "error" {
            Text(bot.lastMessage ?? "Something went wrong")
                .font(.subheadline)
                .foregroundStyle(Palette.danger)
                .lineLimit(2)
        } else {
            Text(bot.lastMessage ?? "\(BackendInfo.name(bot.backend)) · \(bot.folderName)")
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .lineLimit(2)
        }
    }
}

private struct EmptyRoster: View {
    let create: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            CharacterAvatar(shape: "cloud", color: "green", size: 72)
            Text("No bots yet").font(.title3.bold()).foregroundStyle(Palette.text)
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

struct ConnectionBanner: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        switch model.connection {
        case .online, .unpaired:
            EmptyView()
        case .connecting:
            Label("Connecting to \(model.hostName)…", systemImage: "antenna.radiowaves.left.and.right")
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
        case let .offline(reason):
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: "wifi.exclamationmark").foregroundStyle(Palette.warning)
                Text(reason).font(.footnote).foregroundStyle(Palette.secondary)
                Spacer()
                Button("Retry") { model.restartStream() }.font(.footnote.bold())
            }
            .padding(10)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 10))
        }
    }
}
