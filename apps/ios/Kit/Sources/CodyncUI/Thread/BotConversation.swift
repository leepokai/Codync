import CodyncKit
import SwiftUI

/// The peer whose conversation with the chat's bot the sheet shows.
struct BotChatTarget: Identifiable, Equatable {
    let id: String
}

/// The compact line a bot-to-bot message leaves in the chat; tapping it opens the conversation.
struct BotMessageRow: View {
    let group: BotExchangeGroup
    let open: () -> Void
    @Environment(BotStore.self) private var model

    var body: some View {
        let peer = model.bots[group.peerId]
        let name = model.authorName(group.peerId)
        let failed = group.failed
        Button(action: open) {
            HStack(spacing: 6) {
                if failed {
                    Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Palette.danger)
                }
                Text(group.title).foregroundStyle(failed ? Palette.danger : Palette.secondary)
                if let peer { CharacterAvatar(bot: peer, size: 18, animated: false) }
                Text(name).fontWeight(.semibold).foregroundStyle(failed ? Palette.danger : Palette.text)
                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
            }
            .font(.footnote)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        .padding(.top, 2)
        .accessibilityLabel(group.accessibilityLabel(peerName: name))
        .accessibilityHint("Opens the conversation")
    }
}

/// "[avatar] A ⇄ [avatar] B": the two bots of a conversation.
/// The pair a conversation is between, as its sheet's title: the chat's own bot, then its peer.
struct BotPairTitle: View {
    let botId: String
    let peerId: String
    @Environment(BotStore.self) private var model

    var body: some View {
        HStack(spacing: 8) {
            side(botId)
            Image(systemName: "arrow.left.arrow.right").font(.footnote).foregroundStyle(Palette.tertiary)
            side(peerId)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(model.authorName(botId)) and \(model.authorName(peerId))")
    }

    private func side(_ id: String) -> some View {
        HStack(spacing: 6) {
            if let bot = model.bots[id] { CharacterAvatar(bot: bot, size: 22, animated: false) }
            Text(model.authorName(id))
        }
    }
}

/// Read-only history between a bot and one peer, newest at the bottom.
struct BotConversationView: View {
    let botId: String
    let peerId: String
    @Environment(BotStore.self) private var model
    @State private var fetched: [Entry]?
    @State private var error: String?

    private var rows: [BotConversationRow] {
        BotConversationRow.build(BotExchange.conversation(fetched: fetched ?? [], live: model.chat(botId), peer: peerId))
    }

    var body: some View {
        let rows = rows
        VStack(spacing: 0) {
            ModalHeader { BotPairTitle(botId: botId, peerId: peerId) }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 4) {
                    if let error {
                        Text(error).font(.footnote).foregroundStyle(Palette.danger)
                            .frame(maxWidth: .infinity).padding(.top, 24)
                    } else if rows.isEmpty, fetched != nil {
                        Text("No messages yet.").font(.footnote).foregroundStyle(Palette.tertiary)
                            .frame(maxWidth: .infinity).padding(.top, 24)
                    }
                    ForEach(rows) { rowView($0) }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 12)
            }
            .defaultScrollAnchor(.bottom)
        }
        .background(Palette.background)
        .task(id: peerId) {
            do {
                fetched = try await model.botConversation(botId, peer: peerId)
            } catch {
                fetched = []
                self.error = error.localizedDescription
            }
        }
    }

    @ViewBuilder private func rowView(_ row: BotConversationRow) -> some View {
        switch row {
        case .separator(_, let date):
            TimeSeparator(date: date)
        case .message(_, let author, let text, let showsAuthor):
            VStack(alignment: .leading, spacing: 4) {
                if showsAuthor { AuthorLabel(botId: author).padding(.top, 8) }
                ConversationBubble(text: text)
            }
        case .status(_, let outcome, let awaiting):
            caption(outcome, awaiting: awaiting)
        }
    }

    @ViewBuilder private func caption(_ outcome: BotExchange.Outcome, awaiting: String) -> some View {
        switch outcome {
        case .pending:
            Text("Waiting for \(model.authorName(awaiting))…").font(.footnote).foregroundStyle(Palette.secondary)
                .padding(.leading, 12)
        case .failed(let detail):
            Text(detail.isEmpty ? "Didn't complete." : detail).font(.footnote).foregroundStyle(Palette.danger)
                .textSelection(.enabled).padding(.leading, 12)
        case .done:
            EmptyView()
        }
    }
}

/// A message of the conversation: the agent bubble.
private struct ConversationBubble: View {
    let text: String

    var body: some View {
        MarkdownText(text)
            .equatable()
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            .contextActions { [MenuItem("Copy", icon: "square.on.square") { Pasteboard.copy(text) }] }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.trailing, 40)
    }
}
