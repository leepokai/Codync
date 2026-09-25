import CodyncKit
import SwiftUI

public extension View {
    /// The one "Delete <bot>?" confirmation, used wherever a bot can be deleted.
    /// Setting `bot` presents it; `onDeleted` runs after the store deletes it.
    func deleteBotConfirmation(_ bot: Binding<Bot?>, onDeleted: @escaping () -> Void = {}) -> some View {
        modifier(DeleteBotConfirmation(bot: bot, onDeleted: onDeleted))
    }

    /// Shows `BotStore.lastError` in a dialog.
    func storeErrorAlert(_ model: BotStore) -> some View {
        codyncDialog("Something went wrong",
                     isPresented: Binding(get: { model.lastError != nil }, set: { if !$0 { model.lastError = nil } }),
                     message: model.lastError, cancel: "OK") { [] }
    }
}

private struct DeleteBotConfirmation: ViewModifier {
    @Binding var bot: Bot?
    let onDeleted: () -> Void
    @Environment(BotStore.self) private var model

    func body(content: Content) -> some View {
        content.codyncDialog(
            "Delete \(bot?.name ?? "bot")?",
            isPresented: Binding(get: { bot != nil }, set: { if !$0 { bot = nil } }),
            message: "Files it changed on your computer stay as they are."
        ) {
            // Captured now: the dialog clears `bot` before running the action.
            let target = bot
            return [DialogAction("Delete bot and its conversation", destructive: true) {
                if let target { model.delete(target) }
                onDeleted()
            }]
        }
    }
}
