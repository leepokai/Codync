import CodyncKit
import SwiftUI

/// A thread's sheet target: the bot and the thread's root message.
struct ThreadTarget: Identifiable {
    let botId: String
    let root: String
    var id: String { root }
}

/// One thread over the chat: the message it branched from, its replies and a box to continue
/// there. The chat itself keeps everything else.
struct ThreadSheet: View {
    let botId: String
    let root: String
    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var info: ThreadInfo?
    @State private var unread = 0
    @State private var replyingTo: Entry?
    @State private var showTrace = false
    @State private var renaming = false
    @State private var renameText = ""
    @State private var deleting = false
    @State private var following = true

    private var bot: Bot? { model.bots[botId] }
    private var working: Bool { bot?.isWorking(in: botId) == true && !model.isOffline }

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader {
                VStack(alignment: .leading, spacing: 1) {
                    Text(info?.name ?? L10n.tr("Thread"))
                        .lineLimit(1)
                    Text(bot?.name ?? "")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(Palette.tertiary)
                        .lineLimit(1)
                }
            } trailing: {
                DropdownMenu { actions } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(Palette.secondary)
                        .frame(width: 36, height: 36)
                        .contentShape(Rectangle())
                }
                .accessibilityLabel("Thread actions")
            }
            Hairline()
            transcript
        }
        .background(Palette.background)
        .task(id: root) {
            unread = ([model.threads[botId], model.archivedThreads[botId]].compactMap { $0 }.flatMap { $0 }
                .first { $0.id == root }?.unread) ?? 0
            info = await model.readThread(botId, root: root) ?? info
        }
        .codyncSheet(isPresented: $showTrace) { TraceView(botId: botId) }
        .codyncDialog("Rename thread", isPresented: $renaming, field: DialogField("A short name", text: $renameText)) {
            [DialogAction("Save") { model.renameThread(root, botId: botId, title: renameText) }]
        }
        .codyncDialog("Delete thread?", isPresented: $deleting, message: "Its replies go with it; the chat stays.") {
            [DialogAction("Delete thread", destructive: true) {
                model.deleteThread(root, botId: botId)
                dismiss()
            }]
        }
    }

    private var actions: [MenuItem] {
        var items = [
            MenuItem("Rename thread", icon: "pencil") {
                renameText = info?.title ?? ""
                renaming = true
            },
        ]
        if info?.archived == true {
            items.append(MenuItem("Restore", icon: "tray.and.arrow.up") { model.setThreadArchived(root, botId: botId, archived: false) })
        } else {
            items.append(MenuItem("Archive thread", icon: "archivebox") { model.setThreadArchived(root, botId: botId, archived: true) })
        }
        items.append(MenuItem("Delete thread", icon: "trash", destructive: true, divider: true) { deleting = true })
        return items
    }

    private var transcript: some View {
        let replies = model.threadChat(botId, root)
        var rows: [ConversationRow] = []
        if let branch = info?.branchText, !branch.isEmpty {
            rows.append(ConversationRow("branch") { BranchLine(text: branch) { dismiss() } })
        }
        if let entry = rootEntry {
            rows.append(ConversationRow("root") {
                ChatRow(entry: entry, groupStart: true, chat: bot, openTrace: { showTrace = true }, reply: nil)
            })
            rows.append(ConversationRow("count") { CountLine(count: replies.count) })
        }
        let newLineAfter = max(0, replies.count - unread)
        for (index, reply) in replies.enumerated() {
            if unread > 0, index == newLineAfter { rows.append(ConversationRow("new") { UnreadLine() }) }
            rows.append(ConversationRow(reply.id) {
                ChatRow(entry: reply, groupStart: index == 0 || replies[index - 1].kind == "user" || reply.kind == "user",
                        chat: bot, openTrace: { showTrace = true }, reply: nil)
            })
        }
        if let bot, working {
            rows.append(ConversationRow("working") {
                WorkingIndicator(bot: bot, steps: Entry.workingSteps(replies))
                    .padding(.top, 6)
            })
        }
        return VStack(spacing: 0) {
            ConversationList(rows: rows, following: $following) {}
            VStack(spacing: 6) {
                if let mismatch = model.mismatch {
                    UpdateNeededCard(store: model, mismatch: mismatch).padding(.horizontal, 16).padding(.bottom, 8)
                } else {
                    Composer(botId: botId, replyingTo: $replyingTo, thread: root)
                }
            }
            .padding(.top, 6)
        }
    }

    private var rootEntry: Entry? {
        model.chat(botId).first { $0.id == root }
    }
}

/// The message a thread was branched from, and a jump back to it in the chat.
private struct BranchLine: View {
    let text: String
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                Image(systemName: "arrow.turn.down.right")
                    .font(.system(size: 11))
                    .foregroundStyle(Palette.tertiary)
                VStack(alignment: .leading, spacing: 1) {
                    Text(L10n.tr("Branched from"))
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(Palette.tertiary)
                    Text(text)
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(L10n.tr("Branched from")) \(text)")
        .padding(.top, 6)
    }
}

/// How many replies a thread has, above them.
private struct CountLine: View {
    let count: Int

    private var label: String {
        if count == 0 { return L10n.tr("No replies yet") }
        return count == 1 ? L10n.tr("1 reply") : L10n.tr("\(count) replies")
    }

    var body: some View {
        HStack(spacing: 10) {
            Text(label)
                .font(.caption)
                .foregroundStyle(Palette.tertiary)
            Rectangle()
                .fill(Palette.border)
                .frame(height: 1)
        }
        .padding(.vertical, 12)
    }
}

/// Where the unread replies begin.
private struct UnreadLine: View {
    var body: some View {
        HStack(spacing: 8) {
            Text(L10n.tr("New"))
                .font(.caption2.weight(.semibold))
                .foregroundStyle(Palette.accent)
            Rectangle()
                .fill(Palette.accent.opacity(0.4))
                .frame(height: 1)
        }
        .padding(.vertical, 6)
    }
}
