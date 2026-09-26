import CodyncKit
import SwiftUI

/// The message box under a chat or a thread: send, or stop while the bot (or the
/// group) is working there. In a group, typing `@` suggests its bots.
struct Composer: View {
    let botId: String
    /// Replies go to the thread on this message.
    var thread: String?
    @Environment(BotStore.self) private var model
    @State private var draft = ""
    @FocusState private var focused: Bool

    private var bot: Bot? { model.bots[botId] }
    private var working: Bool { bot?.isWorking(in: botId, thread: thread) == true }

    private var canSend: Bool {
        // Offline computers still take messages when the relay can hold them for it.
        !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && (model.connection == .online || model.canQueue)
    }

    private var placeholder: String {
        let name = bot?.name ?? ""
        if thread != nil { return "Reply…" }
        if bot?.isGroup == true { return "Message \(name) · @ to ask one bot" }
        return working ? "Queue a message for \(name)" : "Message \(name)"
    }

    /// The `@partial` being typed at the end, if any.
    private var mentionQuery: String? {
        guard bot?.isGroup == true, let at = draft.lastIndex(of: "@") else { return nil }
        let query = draft[draft.index(after: at)...]
        if at > draft.startIndex, !draft[draft.index(before: at)].isWhitespace { return nil }
        return query.contains(where: \.isNewline) || query.count > 24 ? nil : String(query)
    }

    private var suggestions: [Bot] {
        guard let query = mentionQuery, let bot else { return [] }
        return model.members(of: bot).filter {
            query.isEmpty || $0.name.localizedCaseInsensitiveContains(query)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if !suggestions.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 6) {
                        ForEach(suggestions) { member in
                            Button { mention(member) } label: {
                                HStack(spacing: 6) {
                                    CharacterAvatar(bot: member, size: 18, animated: false)
                                    Text(member.name).font(.subheadline).foregroundStyle(Palette.text)
                                }
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                                .background(Palette.surface, in: Capsule())
                            }
                            .buttonStyle(PressScale())
                        }
                    }
                    .padding(.horizontal, 16)
                }
                .transition(.move(edge: .bottom).combined(with: .opacity))
            }
            field
        }
        .animation(Motion.layout, value: suggestions.map(\.id))
    }

    private var field: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField(placeholder, text: $draft, axis: .vertical)
                .lineLimit(1...8)
                .font(InterfaceMetrics.body)
                .textFieldStyle(.plain)
                .focused($focused)
                .padding(.vertical, InterfaceMetrics.value(mac: 7, mobile: 10))
                .sendOnReturn(submit)
            if working && draft.isEmpty {
                Button {
                    model.stop(botId)
                } label: {
                    Image(systemName: "stop.fill")
                        .font(.system(size: 12, weight: .bold))
                        .frame(width: InterfaceMetrics.value(mac: 28, mobile: 34), height: InterfaceMetrics.value(mac: 28, mobile: 34))
                        .background(Palette.accentFill, in: Circle())
                        .foregroundStyle(Palette.onAccent)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Stop")
                .help("Stop")
            } else {
                Button(action: submit) {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 14, weight: .bold))
                        .frame(width: InterfaceMetrics.value(mac: 28, mobile: 34), height: InterfaceMetrics.value(mac: 28, mobile: 34))
                        .background(canSend ? Palette.accentFill : Palette.accentDim, in: Circle())
                        .foregroundStyle(canSend ? Palette.onAccent : Palette.tertiary)
                }
                .buttonStyle(.plain)
                .disabled(!canSend)
                .accessibilityLabel("Send")
                .help("Send")
            }
        }
        .padding(.leading, InterfaceMetrics.value(mac: 14, mobile: 18))
        .padding(.trailing, 6)
        .padding(.vertical, InterfaceMetrics.value(mac: 4, mobile: 6))
        .composerSurface(in: RoundedRectangle(cornerRadius: 24, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.top, 6)
        .padding(.bottom, 16)
        #if os(macOS)
            .frame(maxWidth: 820)
            .frame(maxWidth: .infinity)
        #endif
    }

    private func mention(_ member: Bot) {
        guard let at = draft.lastIndex(of: "@") else { return }
        draft = String(draft[..<at]) + "@\(member.name) "
        focused = true
    }

    private func submit() {
        guard canSend else { return }
        model.send(draft, to: botId, thread: thread)
        draft = ""
    }
}
