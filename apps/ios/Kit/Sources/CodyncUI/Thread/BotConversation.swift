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
        let name = model.authorName(group.peerId, fallback: group.peerName)
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
    /// The peer's name when it is on another computer.
    let peerName: String?
    @Environment(BotStore.self) private var model

    var body: some View {
        HStack(spacing: 8) {
            side(botId)
            Image(systemName: "arrow.left.arrow.right").font(.footnote).foregroundStyle(Palette.tertiary)
            side(peerId)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(name(botId)) and \(name(peerId))")
    }

    private func name(_ id: String) -> String {
        model.authorName(id, fallback: id == peerId ? peerName : nil)
    }

    private func side(_ id: String) -> some View {
        HStack(spacing: 6) {
            if let bot = model.bots[id] { CharacterAvatar(bot: bot, size: 22, animated: false) }
            Text(name(id))
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

    private var exchanges: [BotExchange] {
        BotExchange.conversation(fetched: fetched ?? [], live: model.chat(botId), peer: peerId)
    }

    var body: some View {
        let exchanges = exchanges
        let rows = BotConversationRow.build(exchanges)
        let peerName = exchanges.peerName
        VStack(spacing: 0) {
            ModalHeader { BotPairTitle(botId: botId, peerId: peerId, peerName: peerName) }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 4) {
                    if let error {
                        Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger)
                            .frame(maxWidth: .infinity).padding(.top, 24)
                    } else if rows.isEmpty, fetched != nil {
                        Text(L10n.tr("No messages yet.")).font(.footnote).foregroundStyle(Palette.tertiary)
                            .frame(maxWidth: .infinity).padding(.top, 24)
                    }
                    ForEach(rows) { rowView($0, peerName: peerName) }
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

    /// A bot of this computer by its own name; the peer on another computer by the name its exchanges carry.
    private func name(_ id: String, peerName: String?) -> String {
        model.authorName(id, fallback: id == peerId ? peerName : nil)
    }

    @ViewBuilder private func rowView(_ row: BotConversationRow, peerName: String?) -> some View {
        switch row {
        case .separator(_, let date):
            TimeSeparator(date: date)
        case .message(_, let author, let text, let showsAuthor):
            VStack(alignment: .leading, spacing: 4) {
                if showsAuthor { AuthorLabel(botId: author, name: name(author, peerName: peerName)).padding(.top, 8) }
                ConversationBubble(text: text)
            }
        case .status(_, let outcome, let awaiting):
            caption(outcome, awaiting: name(awaiting, peerName: peerName))
        }
    }

    @ViewBuilder private func caption(_ outcome: BotExchange.Outcome, awaiting: String) -> some View {
        switch outcome {
        case .pending:
            Text(L10n.tr("Waiting for \(awaiting)…")).font(.footnote).foregroundStyle(Palette.secondary)
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
