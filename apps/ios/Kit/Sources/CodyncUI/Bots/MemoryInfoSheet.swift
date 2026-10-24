import CodyncKit
import SwiftUI

/// The memory screen's info buttons: what the harness's own memory means for the bot, and what dreaming does. The same
/// text as the desktop app's; the terminal UI says it short in place.
struct MemoryInfoSheet: View {
    struct Point {
        let icon: String
        let title: String
        let detail: String
    }

    let title: String
    let points: [Point]

    /// How the bot reaches its harness's own memory.
    enum Access {
        /// A switch, and Codync shows the notes (Claude Code's are a file on the computer).
        case readable
        /// A switch only.
        case hidden
        /// No switch: one memory every bot on the harness shares (Hermes Agent, Gemini CLI).
        case shared
    }

    static func harness(_ harness: String, access: Access) -> MemoryInfoSheet {
        // Translate the sentence, then put the harness name in, so the name stays as written.
        func fill(_ template: String) -> String {
            L10n.tr(template).replacingOccurrences(of: "{name}", with: harness)
        }
        let shared = access == .shared
        let points: [Point?] = [
            Point(icon: "brain", title: "A memory of its own", detail: fill(shared
                ? "{name} keeps notes apart from Codync's memory, and Codync can't turn them off for one bot."
                : "{name} keeps notes apart from Codync's memory. It's off by default, so this bot has one memory and you see all of it here.")),
            shared ? Point(icon: "person.2", title: "Shared by every bot", detail: fill("Every bot that runs {name} reads and writes the same notes, and so does {name} when you use it outside Codync.")) : nil,
            Point(icon: "square.on.square", title: "Remembered twice", detail: "With both on, the bot may save the same thing twice, and the two can disagree after something changes."),
            Point(icon: "trash", title: "Forgetting", detail: fill("What the bot forgets in Codync stays in {name}'s notes. Codync can't change them.")),
            Point(icon: "eye", title: "Where to see it", detail: fill(access == .readable ? "Its notes show read-only under the {name} tab." : "Codync can't show it; look in {name} itself.")),
            shared ? Point(icon: "gearshape", title: "Turning it off", detail: fill("Where {name} has a setting for it, that setting turns it off for every session at once.")) : nil,
        ]
        return MemoryInfoSheet(title: fill("{name}'s own memory"), points: points.compactMap { $0 })
    }

    static let dreams = MemoryInfoSheet(title: "Dreaming", points: [
        Point(icon: "moon", title: "Tidying between chats", detail: "After five quiet minutes or eight messages, or once memory is nearly full, the bot reads what you said since its last run and updates its memory."),
        Point(icon: "plus.circle", title: "Adds and takes out", detail: "Green lines are memories it added. Red lines are ones it took out because they went out of date or moved into a note. It can also add a line to a note."),
        Point(icon: "text.bubble", title: "Only your words", detail: "What it adds quotes something you typed yourself; pasted text, the bot's replies and other bots don't count. It never rewrites what you wrote. It may also put a passed date in the past tense, and before memory fills up it moves detail, word for word, into notes by topic, keeping one line per topic that points to its note."),
        Point(icon: "lock.shield", title: "Kept apart", detail: "It runs in its own session, away from the chat, and fixed checks decide what it may save."),
        Point(icon: "eye", title: "A record", detail: "This tab only shows what it did. To change a memory, tell the bot."),
    ])

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader(title)
            CardForm {
                CardSection {
                    ForEach(points, id: \.title) { item in
                        HStack(alignment: .firstTextBaseline, spacing: 12) {
                            Image(systemName: item.icon)
                                .foregroundStyle(Palette.secondary)
                                .frame(width: 22)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(L10n.tr(item.title))
                                Text(L10n.tr(item.detail))
                                    .font(.footnote)
                                    .foregroundStyle(Palette.secondary)
                                    .fixedSize(horizontal: false, vertical: true)
                            }
                        }
                    }
                }
            }
        }
        .background(Palette.background)
    }
}
