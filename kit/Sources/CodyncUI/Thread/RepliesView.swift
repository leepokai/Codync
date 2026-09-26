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
    @Environment(BotStore.self) private var model
    @State private var showTrace = false

    private var chat: Bot? { model.bots[botId] }
    private var root: Entry? { model.allEntries(botId).first { $0.id == rootId } }

    var body: some View {
        let replies = model.replies(botId, root: rootId)
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Thread").font(InterfaceMetrics.body.weight(.semibold)).foregroundStyle(Palette.text)
                    Text(chat?.name ?? "").font(.caption).foregroundStyle(Palette.tertiary).lineLimit(1)
                }
                Spacer(minLength: 8)
                IconButton("Close thread", systemImage: "xmark", action: close)
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.leading, InterfaceMetrics.value(mac: 16, mobile: 20))
            .padding(.trailing, InterfaceMetrics.value(mac: 10, mobile: 12))
            .frame(height: InterfaceMetrics.value(mac: 48, mobile: 56))
            Rectangle().fill(Palette.border).frame(height: 0.5)

            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if let root {
                        ChatRow(entry: root, groupStart: true, chat: chat) { showTrace = true }
                        HStack(spacing: 10) {
                            Text(replies.isEmpty ? "No replies yet" : replies.count == 1 ? "1 reply" : "\(replies.count) replies")
                                .font(.caption)
                                .foregroundStyle(Palette.tertiary)
                                .fixedSize()
                            Rectangle().fill(Palette.border).frame(height: 1)
                        }
                        .padding(.vertical, 12)
                    }
                    ForEach(ChatItem.build(replies)) { item in
                        if case let .entry(e, groupStart) = item.kind {
                            ChatRow(entry: e, groupStart: groupStart, chat: chat) { showTrace = true }
                        }
                    }
                    if let chat, chat.isWorking(in: botId, thread: rootId), !model.isOffline {
                        WorkingIndicator(bot: chat) { showTrace = true }.padding(.top, 6)
                    }
                    Color.clear.frame(height: 8)
                }
                .padding(.horizontal, 16)
                .padding(.top, 8)
            }
            .defaultScrollAnchor(.bottom)
            .scrollDismissesKeyboard(.interactively)
            .safeAreaInset(edge: .bottom) { Composer(botId: botId, thread: rootId) }
        }
        .background(Palette.background)
        .task(id: rootId) { await model.loadThread(botId, root: rootId) }
        // Replies count as unread too: reading the thread reads them.
        .onAppear { model.markRead(botId) }
        .onChange(of: replies.last?.id) { _, _ in model.markRead(botId) }
        .codyncSheet(isPresented: $showTrace) {
            TraceView(botId: botId, thread: rootId)
                #if os(macOS)
                    .frame(width: 620, height: 560)
                #endif
        }
    }
}

/// Which thread is open.
struct ThreadTarget: Identifiable, Equatable {
    let id: String
}
