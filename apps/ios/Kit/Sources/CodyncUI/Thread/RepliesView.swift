import CodyncKit
import SwiftUI

/// A thread (Slack's "reply in thread"): the message it started on, its replies,
/// and a box to continue there. In a bot's own chat the thread is a separate branch
/// of the conversation (its own session, forked from the chat); in a group the room
/// answers inside the thread.
struct RepliesView: View {
    let botId: String
    let rootId: String
    let close: () -> Void
    @Environment(BotStore.self) var model
    @State var showTrace = false
    @State var following = true

    var chat: Bot? { model.bots[botId] }
    var root: Entry? { model.allEntries(botId).first { $0.id == rootId } }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Thread").font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                    Text(chat?.name ?? "").font(.caption).foregroundStyle(Palette.tertiary).lineLimit(1)
                }
                Spacer(minLength: 8)
                IconButton("Close thread", systemImage: "xmark", action: close)
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.leading, 20)
            .padding(.trailing, 12)
            .frame(height: 56)
            Rectangle().fill(Palette.border).frame(height: 0.5)

            // The replies: each platform's own `messages`, in its `RepliesView+` file.
            messages
                .safeAreaInset(edge: .bottom) {
                    if let mismatch = model.mismatch {
                        UpdateNeededCard(store: model, mismatch: mismatch)
                            .padding(.horizontal, 16)
                            .padding(.bottom, 8)
                    } else {
                        Composer(botId: botId, thread: rootId)
                    }
                }
        }
        .background(Palette.background)
        .task(id: rootId) { await model.loadThread(botId, root: rootId) }
        // A thread is read on its own: having it open reads its replies.
        .readingConversation(botId, thread: rootId)
        .codyncSheet(item: Binding<FileDownloads.Export?>(get: {
            guard let item = model.fileDownloads.export, item.botId == botId, item.threadId == rootId else { return nil }
            return item
        }, set: { if $0 == nil { model.fileDownloads.dismissExport() } })) { item in
            FileExportSheet(url: item.url) { model.fileDownloads.dismissExport() }
        }
        .codyncSheet(isPresented: $showTrace) {
            TraceView(botId: botId, thread: rootId)
        }
    }
}

/// Which thread is open.
struct ThreadTarget: Identifiable, Equatable {
    let id: String
}

extension RepliesView {
    /// iPhone: the replies follow the newest one until the reader scrolls away.
    var messages: some View {
        let replies = model.replies(botId, root: rootId)
        let working = chat?.isWorking(in: botId, thread: rootId) == true && !model.isOffline
        let items = ChatItem.build(replies)
        var rows: [ConversationRow] = []
        if let root {
            // The host's count, as on the thread's chip: messages, not the agent's trace.
            let count = root.data.thread?.count ?? 0
            rows.append(ConversationRow("root") {
                ChatRow(entry: root, groupStart: true, chat: chat) { showTrace = true }
                    .equatable()
            })
            rows.append(ConversationRow("count") {
                HStack(spacing: 10) {
                    Text(count == 0 ? "No replies yet" : count == 1 ? "1 reply" : "\(count) replies")
                        .font(.caption)
                        .foregroundStyle(Palette.tertiary)
                        .fixedSize()
                    Rectangle().fill(Palette.border).frame(height: 1)
                }
                .padding(.vertical, 12)
            })
        }
        for item in items {
            if case let .entry(e, groupStart) = item.kind {
                rows.append(ConversationRow(item.id, isUserMessage: item.isUserMessage) {
                    ChatRow(entry: e, groupStart: groupStart, chat: chat) { showTrace = true }
                        .equatable()
                })
            }
        }
        if let chat, working {
            rows.append(ConversationRow("working") {
                WorkingIndicator(bot: chat, thinking: model.currentThinking(botId, thread: rootId))
                    .padding(.top, 6)
            })
        }
        return ConversationList(rows: rows, following: $following)
    }
}
