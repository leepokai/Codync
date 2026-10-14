import CodyncKit
import SwiftUI

/// Settings' info button beside Share usage analytics: everything it covers. The same text as the
/// desktop app's and the terminal UI's, true to the privacy policy.
public struct AnalyticsSheet: View {
    private static let details: [(icon: String, title: String, detail: String)] = [
        ("chart.bar", "Features you use", "Opening the app, creating or deleting a bot, sending a message (how many attachments, never the files), running a routine or a voice call, installing a connector. Each comes with the app version, the platform, and the bot's agent and model."),
        ("exclamationmark.triangle", "Crashes and errors", "When Codync crashes or an agent fails to answer: the kind of error and where in Codync's own code it happened. Never the error's message; file paths are cut down to Codync's own files."),
        ("globe", "Approximate location", "Country and city, looked up from your IP address. The IP address itself is never stored."),
        ("person.crop.circle", "Who it belongs to", "Your Codync account when you're signed in, otherwise a random ID made on this device."),
        ("eye.slash", "Never shared", "Your messages, prompts, code, files, file names or paths, bot names, credentials, or anything your agents produce."),
        ("lock.shield", "How it's used", "Only to spot bugs sooner and see which features work and where people get stuck. Never sold, never used for ads or to track you across other apps and websites."),
        ("power", "Your choice", "Each iPhone and each computer has its own switch. Turning it off stops sending right away and drops anything not sent yet."),
    ]

    /// The footer, with "Privacy Policy" an underlined link.
    private static let policy: AttributedString = {
        var text = AttributedString("See the Privacy Policy for details.")
        if let range = text.range(of: "Privacy Policy") {
            text[range].link = URL(string: "https://codync.dev/privacy")
            text[range].underlineStyle = .single
        }
        return text
    }()

    public init() {}

    public var body: some View {
        VStack(spacing: 0) {
            ModalHeader("Usage analytics")
            CardForm {
                CardSection {
                    ForEach(Self.details, id: \.title) { item in
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Image(systemName: item.icon)
                                .foregroundStyle(Palette.secondary)
                                .frame(width: 22)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(item.title)
                                Text(item.detail)
                                    .font(.footnote)
                                    .foregroundStyle(Palette.secondary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                }
                Text(Self.policy)
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .tint(Palette.text)
                    .padding(.horizontal, 2)
            }
        }
        .background(Palette.background)
    }
}
