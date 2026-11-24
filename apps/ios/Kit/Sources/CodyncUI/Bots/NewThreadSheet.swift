import CodyncKit
import SwiftUI

/// A thread the user starts with `bot` from the Threads tab: its first message, on the bot's own folder, model and
/// approvals. `started` gets the new task's id once the computer took it and the sheet is gone, so what it opens
/// doesn't push while the sheet still slides away (a push then lands blank).
public struct NewThreadSheet: View {
    let bot: Bot
    let started: (String) -> Void
    @Environment(BotStore.self) private var store
    @Environment(\.dismissModal) private var dismiss
    @State private var text = ""
    @State private var sending = false
    @State private var error: String?
    /// The task the computer took, handed to `started` once the sheet has slid away.
    @State private var taskId: String?
    @FocusState private var focused: Bool

    public init(bot: Bot, started: @escaping (String) -> Void) {
        self.bot = bot
        self.started = started
    }

    private var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader(title: { Text(L10n.tr("New thread with {name}").replacingOccurrences(of: "{name}", with: bot.name)) }) {
                if sending {
                    Spinner()
                } else {
                    IconButton("Send", systemImage: "arrow.up", action: send)
                        .disabled(trimmed.isEmpty)
                }
            }
            CardForm {
                CardSection {
                    TextField(L10n.tr("Message"), text: $text, axis: .vertical)
                        .lineLimit(3...10)
                        .focused($focused)
                }
                if let error {
                    Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
                }
            }
            .textFieldStyle(.plain)
        }
        .background(Palette.background)
        .presentationDetents([.medium])
        .onAppear { focused = true }
        .onDisappear { if let taskId { started(taskId) } }
    }

    private func send() {
        guard !sending, !trimmed.isEmpty else { return }
        sending = true
        error = nil
        Task {
            do {
                taskId = try await store.startThread(botId: bot.id, text: trimmed)
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
            sending = false
        }
    }
}
