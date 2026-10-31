import CodyncKit
import CodyncUI
import SwiftUI

/// The item a lifted row carries (nothing): the system lets go of it when the drag or the
/// menu ends.
final class LiftItem: NSItemProvider {
    private let ended: @Sendable () -> Void

    init(ended: @escaping @Sendable () -> Void) {
        self.ended = ended
        super.init()
    }

    deinit { ended() }
}

/// One row in the roster: a computer's heading or a bot, which the user can drag, or a section's
/// heading or the "Bots" label on a computer (its id carried along: `id` can't read the main-actor store).
enum RosterLine: Identifiable {
    case computer(ComputerID, BotStore)
    case section(RosterSection, ComputerID, BotStore)
    case label(ComputerID)
    case bot(RosterItem, BotStore)

    var id: String {
        switch self {
        case let .computer(id, _): "computer:\(id)"
        case let .section(section, computerId, _): "section:\(computerId):\(section.name)"
        case let .label(computerId): "label:\(computerId)"
        case let .bot(item, _): "bot:\(item.ref.computerId):\(item.bot.id)"
        }
    }

    /// The computer a heading stands for.
    var computerId: ComputerID? {
        if case let .computer(id, _) = self { id } else { nil }
    }

    var sectionComputer: ComputerID? {
        switch self {
        case let .section(_, id, _), let .label(id): id
        default: nil
        }
    }

    func sectionName(on computer: ComputerID) -> String? {
        guard sectionComputer == computer else { return nil }
        return switch self {
        case let .section(section, _, _): section.name
        case .label: ""
        default: nil
        }
    }

    func botId(on computerId: ComputerID) -> String? {
        if case let .bot(item, _) = self, item.ref.computerId == computerId { item.bot.id } else { nil }
    }
}

struct NewTarget: Identifiable {
    enum Kind { case bot, group }
    let id = UUID()
    let kind: Kind
    let computerId: ComputerID
}

struct GroupTarget: Identifiable {
    let id = UUID()
    let computerId: ComputerID
    let group: Bot
}

struct EditTarget: Identifiable {
    let id = UUID()
    let computerId: ComputerID
    let draft: BotDraft
}

