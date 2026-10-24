# iOS Shortcuts

Codync exposes two actions in Apple's Shortcuts app:

- **Open Live Mode**: choose a **Bot**, open its conversation, and start the existing voice chat with the configured voice provider.
- **Send Text to Bot**: choose a **Bot** and supply **Text**. **Open Chat** defaults to off: the message is sent in the background without opening Codync or selecting/reading a conversation. Turn it on to open the chosen chat. The action waits for the computer to accept the message. When the computer is offline and its relay mailbox is available, it confirms that the message was queued instead.

Open Codync and connect a computer first so its bots are available in the picker. In Shortcuts, add an action, search for Codync, and select either action. The Bot parameter can be a saved selection, a compatible shortcut variable, or **Ask Each Time**. Text can be literal text, another action's output, **Shortcut Input**, or **Ask Each Time**. These are two separate actions; no manual URL construction or Bot ID is needed.

Live brings Codync to the foreground. The text action only brings it forward when **Open Chat** is on; on iOS versions before 26, the system may ask to continue in the app. Live uses the normal microphone/speech permission flow; first use may require permission and unlocking the iPhone. Siri phrases are “Open Live Mode in Codync” and “Send text with Codync” (use the installed app name, including Dev). Siri and lock-screen behavior depend on the device and its system settings; a simulator run does not verify lock-screen use.

Only visible bots on computers the currently selected account can reach are offered. Stored selections carry the account scope and computer identity; switching accounts, removing the computer, or deleting/hiding the bot requires choosing a current bot again. Bots with the same name show their computer name. An empty list means Codync has no cached eligible bots yet: open Codync and let the computer sync.

The text action uses the existing message path, including nonce-based retry and the relay mailbox. It preserves the conversation's unsent draft. Empty input and delivery failures throw an error rather than report success. Canceling the action stops pending delivery retries; it cannot retract a message already accepted by the computer or mailbox. Live waits for the connection and bot catch-up; an offline computer produces an error rather than starting a disconnected call. Leaving the selected conversation invalidates its pending Live request, including a request still waiting for the connection. Returning to that bot later does not start an abandoned call.

## Ownership and platforms

The adapter lives in the iOS app (`BotShortcuts` and `AppStore+Shortcuts`); the account-scoped entity query lives in CodyncKit. CodyncUI reuses BotStore's sending and connection logic and delivers a one-shot voice request to ThreadView. A pending shortcut send retains its target connection across background transitions, then releases it after acceptance, failure or cancellation; active calls retain their own connection. Background launches wait for account restoration before resolving the target. The host continues to own message handling and agent work; there is no new host method or compatibility-floor change.

This is an iOS system integration. Desktop already offers bot selection, text sending and voice calls in ThreadView/CallView; Android exposes the same existing functions through ChatScreen and its call bar. Neither client runs iOS App Intents. The terminal UI retains its existing bot selection and message sending and has no microphone voice UI. Their shared behavior and layouts are unchanged.
