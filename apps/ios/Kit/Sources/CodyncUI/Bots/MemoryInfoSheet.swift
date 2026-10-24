import CodyncKit
import SwiftUI

/// The memory screen's info buttons: what the harness's own memory switch means, and what dreaming does. The same
/// text as the desktop app's; the terminal UI says it short in place.
struct MemoryInfoSheet: View {
    struct Point {
        let icon: String
        let title: String
        let detail: String
    }

    let title: String
    let points: [Point]

    /// `readable`: Codync can show the harness's notes (Claude Code's are a file on the computer).
    static func harness(_ harness: String, readable: Bool) -> MemoryInfoSheet {
        // Translate the sentence, then put the harness name in, so the name stays as written.
        func fill(_ template: String) -> String {
            L10n.tr(template).replacingOccurrences(of: "{name}", with: harness)
        }
        return MemoryInfoSheet(title: fill("{name}'s own memory"), points: [
            Point(icon: "brain", title: "A memory of its own", detail: fill("{name} keeps notes apart from Codync's memory. It's off by default, so this bot has one memory and you see all of it here.")),
            Point(icon: "square.on.square", title: "Remembered twice", detail: "With both on, the bot may save the same thing twice, and the two can disagree after something changes."),
            Point(icon: "trash", title: "Forgetting", detail: fill("What the bot forgets in Codync stays in {name}'s notes. Codync can't change them.")),
            Point(icon: "eye", title: "Where to see it", detail: fill(readable ? "Its notes show read-only under the {name} tab." : "Codync can't show it; look in {name} itself.")),
        ])
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
