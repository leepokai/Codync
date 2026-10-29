import CodyncKit
import SwiftUI

/// A voice call's line in the chat: its title (written once the call ends) and length; tapping it opens the call.
struct CallLineRow: View {
    let entry: Entry
    let open: () -> Void

    var body: some View {
        let title = L10n.tr(entry.data.text ?? "Voice chat")
        let length = entry.data.callSeconds.map { String(format: " · %02d:%02d", $0 / 60, $0 % 60) } ?? ""
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
                LazyVStack(alignment: .leading, spacing: 10) {
                    if let error {
                        Text(L10n.tr(error)).font(.footnote).foregroundStyle(Palette.danger)
                            .frame(maxWidth: .infinity).padding(.top, 24)
                    } else if fetched == nil, line.data.call != nil {
                        Spinner(size: 14).frame(maxWidth: .infinity).padding(.top, 24)
                            .accessibilityLabel(L10n.tr("Loading the call"))
                    } else if messages.isEmpty {
                        Text(L10n.tr("Nothing was said on this call.")).font(.footnote).foregroundStyle(Palette.tertiary)
                            .frame(maxWidth: .infinity).padding(.top, 24)
                    }
                    ForEach(Array(messages.enumerated()), id: \.offset) { _, message in row(message) }
                }
                .padding(.horizontal, 16)
                .padding(.bottom, 12)
            }
            .defaultScrollAnchor(.bottom)
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
                .padding(.horizontal, 14).padding(.vertical, 9)
                .background(Palette.bubbleUser, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                .frame(maxWidth: .infinity, alignment: .trailing)
                .padding(.leading, 40)
        case .voice:
            Text(message.text)
                .foregroundStyle(Palette.secondary)
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
