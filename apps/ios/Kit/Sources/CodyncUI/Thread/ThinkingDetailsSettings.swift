import CodyncKit
import SwiftUI

/// A device preference: the working pill unfolds the turn's thinking and steps. Off (the default) keeps the chat to its
/// messages.
enum ThinkingDetails {
    static let storageKey = "showThinkingDetails"
}

/// Settings › Chat: whether the working pill unfolds the turn's thinking and steps on this device.
public struct ThinkingDetailsSettings: View {
    @AppStorage(ThinkingDetails.storageKey) private var details = false

    public init() {}

    public var body: some View {
        CardSection("Chat", footer: "Lets you unfold a working bot's thinking and steps from its status in the chat. Only changes this device.") {
            Toggle(isOn: $details) { Text(L10n.tr("Show thinking details")) }
                .toggleStyle(.codync)
        }
    }
}
