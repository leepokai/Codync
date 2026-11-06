import AppIntents
import CodyncKit
import CodyncUI

struct OpenLiveModeIntent: AppIntent {
    static let title: LocalizedStringResource = "Open Live Mode"
    static let description = IntentDescription("Open a bot's conversation and start voice chat; with no bot chosen, your lead bot, which speaks first. Connect a computer in Codync first.")
    static let openAppWhenRun = true

    /// None: the lead bot.
    @Parameter(title: "Bot")
    var bot: ShortcutBotEntity?

    static var parameterSummary: some ParameterSummary {
        Summary("Open Live Mode with \(\.$bot)")
    }

    @MainActor func perform() async throws -> some IntentResult {
        // No lead to call (an older computer, or one not synced yet): ask, as the action did before leads.
        guard let target = bot ?? ShortcutBotQuery.lead(in: SharedStore.activeContext) else {
            throw $bot.needsValueError("Which bot would you like to talk to?")
        }
        try await AppStore.shared.openShortcutLive(target)
        return .result()
    }
}

struct SendTextToBotIntent: ForegroundContinuableIntent {
    static let title: LocalizedStringResource = "Send Text to Bot"
    static let description = IntentDescription("Send text to a bot in the background. Optionally open its conversation. Finishes when the computer or its offline mailbox accepts the message.")
    static let openAppWhenRun = false
    /// Runs in the background, also from Siri: a locked iPhone must not instruct a coding agent.
    static let authenticationPolicy: IntentAuthenticationPolicy = .requiresAuthentication

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
        AppShortcut(intent: OpenLiveModeIntent(), phrases: ["Open Live Mode in \(.applicationName)", "Talk to \(.applicationName)"],
                    shortTitle: "Open Live Mode", systemImageName: "waveform")
        AppShortcut(intent: SendTextToBotIntent(), phrases: ["Send text with \(.applicationName)"],
                    shortTitle: "Send Text to Bot", systemImageName: "paperplane")
    }
}
