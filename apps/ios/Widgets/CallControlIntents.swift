import AppIntents

// The call's Live Activity buttons. A Live Activity intent runs in the app without opening it, so these are built into
// the app as well and run there; the app sets `run` at launch, and the widget extension only shows the buttons.

/// Mutes the call with a bot, or unmutes it.
struct MuteCallIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Mute Call"
    static let isDiscoverable = false

    @MainActor static var run: (String) -> Void = { _ in }

    @Parameter(title: "Bot") var botId: String

    init() {}

    init(botId: String) { self.botId = botId }

    @MainActor func perform() async throws -> some IntentResult {
        Self.run(botId)
        return .result()
    }
}

/// Ends the call with a bot.
struct EndCallIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "End Call"
    static let isDiscoverable = false

    @MainActor static var run: (String) -> Void = { _ in }

    @Parameter(title: "Bot") var botId: String

    init() {}

    init(botId: String) { self.botId = botId }

    @MainActor func perform() async throws -> some IntentResult {
        Self.run(botId)
        return .result()
    }
}
