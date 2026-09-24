import CodyncKit
import SwiftUI

public extension View {
    /// The one "Delete <bot>?" confirmation, used wherever a bot can be deleted.
    /// Setting `bot` presents it; `onDeleted` runs after the store deletes it.
    func deleteBotConfirmation(_ bot: Binding<Bot?>, onDeleted: @escaping () -> Void = {}) -> some View {
        modifier(DeleteBotConfirmation(bot: bot, onDeleted: onDeleted))
    }

    /// Shows `BotStore.lastError` as an alert.
    func storeErrorAlert(_ model: BotStore) -> some View {
        alert("Something went wrong", isPresented: Binding(get: { model.lastError != nil }, set: { if !$0 { model.lastError = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(model.lastError ?? "")
        }
    }
}

private struct DeleteBotConfirmation: ViewModifier {
    @Binding var bot: Bot?
    let onDeleted: () -> Void
    @Environment(BotStore.self) private var model

    func body(content: Content) -> some View {
        content.confirmationDialog(
            "Delete \(bot?.name ?? "bot")?",
            isPresented: Binding(get: { bot != nil }, set: { if !$0 { bot = nil } }),
            titleVisibility: .visible
        ) {
            Button("Delete bot and its conversation", role: .destructive) {
                if let bot { model.delete(bot) }
                onDeleted()
            }
        } message: {
            Text("Files it changed on your computer stay as they are.")
        }
    }
}
