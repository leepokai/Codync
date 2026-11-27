import CodyncKit
import SwiftUI

/// What a thread's agent runs in the background (a workflow, a command, a monitor), in one capsule directly above the
/// thread's composer, as the bot's work is over its chat (`WorkCapsule`): what the one piece does, or the names of
/// several, and for how long. Its task waits on that work, so the thread stays working between turns.
struct BackgroundCapsule: View {
    let label: (text: String, since: Int64)

    var body: some View {
        HStack(spacing: 8) {
            Spinner(size: 12)
            Text("\(L10n.tr("Background")) · \(label.text)")
                .font(.subheadline)
                .foregroundStyle(Palette.text)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
            Text(Date(milliseconds: label.since), style: .timer)
                .font(.footnote.monospacedDigit())
                .foregroundStyle(Palette.tertiary)
        }
        .frame(minHeight: 36)
        .padding(.horizontal, 14)
        .glass(in: Capsule())
        .padding(.horizontal, 16)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("background-capsule")
    }
}
