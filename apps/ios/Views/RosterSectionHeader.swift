import CodyncKit
import CodyncUI
import SwiftUI

/// A user-made section's heading in the roster, in `ComputerSection`'s look (name, bot count, a
/// trailing chevron): tap folds its bots away on this device; folded, it shows how many unread
/// messages its bots hold.
struct RosterSectionHeader: View {
    let section: RosterSection
    /// Whether touch and hold opens its menu (rename, remove): not on a computer without sections.
    let hasMenu: Bool
    let toggle: () -> Void
    var moving = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var showsUnread: Bool { (section.folded || moving) && section.unread > 0 }

    var body: some View {
        HStack(spacing: 6) {
            // The name gives way first: the count, badge and chevron keep their room.
            Text(section.name)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
            Text("\(section.count)")
                .font(.footnote)
                .foregroundStyle(Palette.tertiary)
                .layoutPriority(1)
            Spacer(minLength: 8)
            // Fades however it changes: a message arriving while folded isn't animated by the store.
            ZStack {
                if showsUnread { UnreadBadge(count: section.unread).transition(.opacity) }
            }
            .animation(Motion.reduced(Motion.fade, reduceMotion), value: showsUnread)
            .layoutPriority(1)
            Image(systemName: "chevron.down")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.tertiary)
                .rotationEffect(.degrees((section.folded || moving) ? -90 : 0))
                .accessibilityHidden(true)
        }
        // `ComputerSection`'s 14 + 6 points above and 2 + 6 below: about 46 points to tap.
        .padding(.top, 20)
        .padding(.bottom, 8)
        .contentShape(Rectangle())
        .onTapGesture(perform: toggle)
        .accessibilityElement(children: .ignore)
        .accessibilityAddTraits([.isHeader, .isButton])
        .accessibilityLabel(spoken)
        .accessibilityValue((section.folded || moving) ? L10n.tr("Collapsed") : L10n.tr("Expanded"))
        .accessibilityHint(hasMenu ? L10n.tr("Double-tap to fold or expand. Touch and hold to rename or remove the section.")
            : L10n.tr("Double-tap to fold or expand."))
        .accessibilityAction(named: (section.folded || moving) ? L10n.tr("Expand") : L10n.tr("Collapse"), toggle)
        .accessibilityAction(.default) { toggle() }
    }

    /// "Work, 2 bots, 3 unread": what the count and the badge stand for, its phrase translated
    /// before the name goes in.
    private var spoken: String {
        let phrase = switch (section.count == 1, showsUnread) {
        case (true, false): L10n.tr("{name}, 1 bot")
        case (false, false): L10n.tr("{name}, {count} bots")
        case (true, true): L10n.tr("{name}, 1 bot, {unread} unread")
        case (false, true): L10n.tr("{name}, {count} bots, {unread} unread")
        }
        // The name goes in last, so one that reads "{count}" stays as the user typed it.
        return phrase.replacingOccurrences(of: "{count}", with: String(section.count))
            .replacingOccurrences(of: "{unread}", with: String(section.unread))
            .replacingOccurrences(of: "{name}", with: section.name)
    }
}

/// The plain "Bots" label over the bots in no section, set like a section heading's name so the
/// rows line up; it folds nothing and has no menu.
struct RosterBotsLabel: View {
    var body: some View {
        Text(L10n.tr("Bots"))
            .font(.footnote.weight(.semibold))
            .foregroundStyle(Palette.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, 20)
            .padding(.bottom, 8)
            .accessibilityAddTraits(.isHeader)
    }
}
