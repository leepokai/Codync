import CodyncKit
import Foundation

/// A user-made section's heading in a computer's roster ("Work"): how many of its bots it lists,
/// their unread messages (shown while it's folded) and whether it's folded on this device.
public struct RosterSection: Sendable {
    public let name: String
    public let count: Int
    public let unread: Int
    public let folded: Bool

    /// Section names as the roster lists them: the way the user's language sorts ("İş" in Turkish),
    /// "Work" before "work" so two names that differ only in case keep one order.
    static func sorted(_ names: some Sequence<String>) -> [String] {
        names.sorted {
            let order = $0.localizedStandardCompare($1)
            return order == .orderedSame ? $0 < $1 : order == .orderedAscending
        }
    }

    /// The section a typed name names: trimmed, nil when nothing but whitespace or control
    /// characters was typed (the host would clear the section or refuse it).
    public static func name(typed: String) -> String? {
        let name = typed.trimmingCharacters(in: .whitespacesAndNewlines.union(.controlCharacters))
        return name.isEmpty ? nil : name
    }

    /// The most Unicode scalars a section name may have, as the host counts them.
    static let maxNameLength = 40

    /// What a name box may hold: at most `maxNameLength` scalars; a name within that (one the host
    /// accepted) is never cut.
    public static func limited(_ typed: String) -> String {
        typed.unicodeScalars.count > maxNameLength
            ? String(String.UnicodeScalarView(typed.unicodeScalars.prefix(maxNameLength))) : typed
    }

    /// This device's folded sections after it renames section `from` to `to` (nil removes it): a
    /// folded `from` stays folded as `to`, so a merge folds the section it joins; an unfolded one
    /// changes nothing.
    public static func folds(_ folded: Set<String>, renaming from: String, to: String?) -> Set<String> {
        guard folded.contains(from) else { return folded }
        var names = folded
        names.remove(from)
        if let to { names.insert(to) }
        return names
    }

    /// A computer's roster (in `BotStore.roster` order) as the rows on screen: pinned bots, then
    /// each section by name with its bots (none while folded), then the bots in no section, under
    /// the "Bots" label when there are sections. A pinned bot shows only on top, whatever its
    /// section; every part keeps the roster's order.
    public static func lines<Item>(_ roster: [Item], folded: Set<String>, bot: (Item) -> Bot) -> [RosterSectionLine<Item>] {
        var pinned: [Item] = [], loose: [Item] = [], bySection: [String: [Item]] = [:]
        for item in roster {
            let placed = bot(item)
            if placed.pinned {
                pinned.append(item)
            } else if let name = placed.section {
                bySection[name, default: []].append(item)
            } else {
                loose.append(item)
            }
        }
        var lines = pinned.map { RosterSectionLine.bot($0) }
        for name in sorted(bySection.keys) {
            let items = bySection[name] ?? []
            let isFolded = folded.contains(name)
            let unread = items.reduce(0) { $0 + bot($1).unread }
            lines.append(.header(RosterSection(name: name, count: items.count, unread: unread, folded: isFolded)))
            if !isFolded { lines += items.map { RosterSectionLine.bot($0) } }
        }
        if !bySection.isEmpty && !loose.isEmpty { lines.append(.label) }
        return lines + loose.map { RosterSectionLine.bot($0) }
    }
}

/// One row of a computer's roster split into sections; `label` is the "Bots" label over the bots in none.
public enum RosterSectionLine<Item> {
    case header(RosterSection)
    case label
    case bot(Item)
}

extension Bot {
    /// Whether the user can drag this bot past `other`: pinned bots share the top whatever their
    /// sections; the rest move within their own section, or among the bots in none.
    func isRosterPeer(of other: Bot) -> Bool {
        pinned == other.pinned && (pinned || section == other.section)
    }
}
