import AppIntents
import CodyncKit
import CodyncUI

struct OpenLiveModeIntent: AppIntent {
    static let title: LocalizedStringResource = "Open Live Mode"
    static let description = IntentDescription("Open a bot's conversation and start voice chat. Connect a computer in Codync first.")
    static let openAppWhenRun = true

    @Parameter(title: "Bot", requestValueDialog: "Which bot would you like to talk to?")
    var bot: ShortcutBotEntity

    static var parameterSummary: some ParameterSummary {
        Summary("Open Live Mode with \(\.$bot)")
    }

    @MainActor func perform() async throws -> some IntentResult {
        let store = try AppStore.shared.openShortcutBot(bot)
        guard let ref = bot.reference(in: AppStore.shared.storage) else { throw HostError.unauthorized("Choose a bot from the current account.") }
        try await store.requestVoiceCall(to: ref.botId)
        return .result()
    }
}

struct SendTextToBotIntent: ForegroundContinuableIntent {
    static let title: LocalizedStringResource = "Send Text to Bot"
    static let description = IntentDescription("Send text to a bot in the background. Optionally open its conversation. Finishes when the computer or its offline mailbox accepts the message.")
    static let openAppWhenRun = false

    @available(iOS 26.0, *)
    static var supportedModes: IntentModes { [.background, .foreground(.dynamic)] }

    @Parameter(title: "Bot", requestValueDialog: "Which bot should receive the text?")
    var bot: ShortcutBotEntity

    @Parameter(title: "Text", requestValueDialog: "What would you like to send?")
    var text: String

    @Parameter(title: "Open Chat", default: false)
    var openChat: Bool

    static var parameterSummary: some ParameterSummary {
        Summary("Send \(\.$text) to \(\.$bot)") { \.$openChat }
    }

    @MainActor func perform() async throws -> some IntentResult & ProvidesDialog {
        if openChat {
            if #available(iOS 26.0, *) {
                try await continueInForeground(alwaysConfirm: false)
            } else {
                try await requestToContinueInForeground()
            }
        }
        let progress = try await AppStore.shared.sendShortcutText(text, to: bot, openChat: openChat)
        if case .queued = progress { return .result(dialog: "Message queued for the offline computer.") }
        return .result(dialog: "Message sent.")
    }
}

struct BotShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: OpenLiveModeIntent(), phrases: ["Open Live Mode in \(.applicationName)"],
                    shortTitle: "Open Live Mode", systemImageName: "waveform")
        AppShortcut(intent: SendTextToBotIntent(), phrases: ["Send text with \(.applicationName)"],
                    shortTitle: "Send Text to Bot", systemImageName: "paperplane")
    }
}
