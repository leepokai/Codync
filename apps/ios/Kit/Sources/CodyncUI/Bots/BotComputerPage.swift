import CodyncKit
import SwiftUI

/// The details sheet's Computer page: the computer the bot runs on, its state and a live picture of its
/// screen to watch the bot work, and whether the bot may use the computer.
struct BotComputerPage: View {
    let botId: String
    @Environment(BotStore.self) private var model

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 12) {
                header
                ScreenPreview(botId: botId)
            }
            .outlinedCard()
            if let screen = model.screen, let bot = model.bots[botId] {
                SwitchRow("Use the computer",
                          detail: screen.controlReady ? nil : "On that computer, open Codync → Settings → Computer access to set it up first.",
                          isOn: Binding(get: { bot.computer }, set: { model.setComputer(bot, $0) }))
                    .disabled(model.isOffline)
                    .outlinedCard()
            }
        }
        .tracked(.screenViewed(.botComputer))
    }

    private var header: some View {
        HStack(spacing: 8) {
            ComputerBadge(model.computer, size: 20)
            Text(model.hostName)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(Palette.text)
                .lineLimit(1)
            Spacer(minLength: 8)
            Text(L10n.tr(model.connectionLabel))
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
        }
    }
}
