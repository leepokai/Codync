import CodyncKit
import SwiftUI

/// A voice call's line in the chat: its title (written once the call ends) and length; tapping it opens the call.
struct CallLineRow: View {
    let entry: Entry
    let open: () -> Void

    var body: some View {
        let title = L10n.tr(entry.data.text ?? "Voice chat")
        let length = callLength(entry.data.callSeconds)
        Button(action: open) {
            HStack(spacing: 6) {
                Image(systemName: "waveform").foregroundStyle(Palette.secondary)
                Text(title).foregroundStyle(Palette.text)
                Text(length).monospacedDigit().foregroundStyle(Palette.secondary)
                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Palette.tertiary)
            }
            .font(.footnote)
            .lineLimit(1)
            .padding(.vertical, 6)
            .frame(maxWidth: .infinity)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        .accessibilityLabel("\(title)\(length)")
        .accessibilityHint("Opens the call")
    }
}

/// What a voice call said, oldest first: the user's and the voice's words, and the messages it carried to the bot
/// and back, which the chat itself leaves out.
struct CallConversationView: View {
    let line: Entry
    @Environment(BotStore.self) private var model
    /// The call's own messages, which the chat this device holds may not reach back to; nil while they load.
    @State private var fetched: [Entry]?
    @State private var error: String?

    var body: some View {
        let entry = model.chat(line.botId).first { $0.id == line.id } ?? line
        let messages = model.chat(line.botId).withCallEntries(fetched ?? []).callConversation(entry)
        VStack(spacing: 0) {
            ModalHeader(entry.data.text ?? "Voice chat")
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    CallHeader(line: entry)
                    Group {
                        if let error {
                            Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger)
                        } else if fetched == nil, line.data.call != nil {
                            Spinner(size: 14).accessibilityLabel(L10n.tr("Loading the call"))
                        } else if messages.isEmpty {
                            Text(L10n.tr("Nothing was said on this call.")).font(.footnote).foregroundStyle(Palette.tertiary)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.bottom, 16)
                    ForEach(Array(messages.enumerated()), id: \.offset) { i, message in
                        row(message).padding(.top, i == 0 ? 0 : message.gap(after: messages[i - 1]))
                    }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
            }
            // A short call reads from the top; a long one opens at its end.
            .defaultScrollAnchor(.top, for: .alignment)
            .defaultScrollAnchor(.bottom, for: .initialOffset)
            .defaultScrollAnchor(.bottom, for: .sizeChanges)
        }
        .background(Palette.background)
        .task(id: line.id) {
            guard let call = line.data.call else { return }
            do {
                fetched = try await model.callEntries(line.botId, call: call)
            } catch {
                fetched = []
                self.error = error.localizedDescription
            }
        }
    }

    @ViewBuilder private func row(_ message: CallMessage) -> some View {
        switch message.speaker {
        case .user:
            Text(message.text)
                .foregroundStyle(Palette.text)
                .padding(.horizontal, 16).padding(.vertical, 10)
                .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .frame(maxWidth: .infinity, alignment: .trailing)
                .padding(.leading, 56)
        case .voice:
            Text(message.text)
                .foregroundStyle(Palette.text)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.trailing, 40)
        case .voiceToBot:
            Label(message.text, systemImage: "arrow.turn.down.right")
                .font(.footnote)
                .foregroundStyle(Palette.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
        case .bot:
            MarkdownText(message.text)
                .equatable()
                .padding(.horizontal, 16).padding(.vertical, 10)
                .background(Palette.bubbleAgent, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.trailing, 40)
        }
    }
}

/// Who the call was with, when it ended and how long it lasted, above what was said.
private struct CallHeader: View {
    let line: Entry
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(spacing: 4) {
            if let bot = model.bots[line.botId] {
                BotAvatar(bot: bot, members: model.members(of: bot), size: 52, animated: false)
                    .padding(.bottom, 8)
                Text(bot.name).font(.headline).foregroundStyle(Palette.text)
            }
            Text(RelativeTime.separator(Date(milliseconds: line.createdAt)) + callLength(line.data.callSeconds))
                .font(.footnote)
                .monospacedDigit()
                .foregroundStyle(Palette.tertiary)
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 12)
        .padding(.bottom, 28)
        .accessibilityElement(children: .combine)
    }
}

private extension CallMessage {
    /// The space above this message: wide where you start a new exchange, close within one.
    func gap(after previous: CallMessage) -> CGFloat {
        speaker == .user && previous.speaker != .user ? 24 : 8
    }
}

/// A call's length after its title or time: " · 00:16"; empty while it has none.
func callLength(_ seconds: Int?) -> String {
    seconds.map { String(format: " · %02d:%02d", $0 / 60, $0 % 60) } ?? ""
}
