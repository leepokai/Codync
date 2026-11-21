import CodyncKit
import SwiftUI

/// The user's messages waiting behind the bot's work, above the composer: each one line, sent now into the running
/// turn (`steer`) or taken back. An older computer has neither action, so the rows only show. Under a task's thread
/// (`workThread`) they are that thread's.
struct QueuedMessages: View {
    let entries: [Entry]
    @Environment(BotStore.self) private var model
    @Environment(\.workThread) private var thread
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        // A long queue scrolls, so every row stays in reach and the chat keeps its room.
        ScrollViewReader { proxy in
            CappedHeight(max: 190) {
                ScrollView {
                    VStack(spacing: 0) {
                        ForEach(entries) { entry in
                            row(entry).id(entry.id).transition(.move(edge: .bottom).combined(with: .opacity))
                        }
                    }
                    .padding(.vertical, 2)
                }
                .scrollIndicators(.hidden)
                .scrollBounceBehavior(.basedOnSize)
            }
            // It stays where it is, the oldest (next to go) in view; a message sent from here shows that it arrived.
            .onChange(of: entries.last?.id) {
                guard let newest = entries.last, let nonce = newest.data.clientNonce, nonce == model.lastSentNonce else { return }
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { proxy.scrollTo(newest.id, anchor: .bottom) }
            }
        }
        .background(Palette.surface)
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .animation(Motion.reduced(Motion.layout, reduceMotion), value: entries.map(\.id))
    }

    private func row(_ entry: Entry) -> some View {
        let files = entry.data.attachments ?? []
        return HStack(spacing: 2) {
            // Its files show as a symbol, and by name when it has no words.
            if !files.isEmpty {
                Image(systemName: files.allSatisfy(\.isImage) ? "photo" : "doc")
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .accessibilityHidden(true)
                    .padding(.trailing, 4)
            }
            Text(MessagePreview.text(entry.replyQuote.text.isEmpty ? files.map(\.name).joined(separator: ", ") : entry.replyQuote.text))
                .font(.subheadline)
                .foregroundStyle(Palette.text)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, model.supportsQueueActions ? 0 : 9)
            if model.supportsQueueActions {
                Group {
                    IconButton("Send now", systemImage: "arrow.turn.down.right") { model.steer(entry, in: thread) }
                    IconButton("Remove", systemImage: "trash") { model.unqueue(entry, in: thread) }
                }
                .disabled(model.answering[entry.id] != nil)
            }
        }
        .padding(.leading, 14)
        .padding(.trailing, 4)
    }
}
