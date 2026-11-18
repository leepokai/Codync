import CodyncKit
import SwiftUI

/// A message's long-press menu: its quick reactions, then Copy, then the rest (Reply, Show what it did…). A picture
/// in the message, in the chat or its viewer, shows the same menu with Copy image and Copy name in Copy's place.
/// Its text's menu has Select Text right after Copy (`textItems`).
struct MessageMenu {
    let entry: Entry
    let reactions: ReactionPick
    /// What follows Copy.
    let actions: [MenuItem]
    /// Opens the message's text to reply to a part of it; nil where the row doesn't offer it.
    var selectText: (() -> Void)?

    /// The plain Copy: the message's text.
    var textCopy: [MenuItem] {
        [MenuItem("Copy", icon: "square.on.square") { Pasteboard.copy(entry.data.text) }]
    }

    func items(copy: [MenuItem]) -> [MenuItem] { copy + actions }

    /// A message's text: Copy, Select Text when the row offers it, then `more` and the rest.
    func textItems(more: [MenuItem] = []) -> [MenuItem] {
        let select = selectText.map { [MenuItem("Select Text", icon: "character.cursor.ibeam", action: $0)] } ?? []
        return items(copy: textCopy + select + more)
    }

    /// The menu of a message's row: Reply where it takes one, then `more`; Select Text where `offersSelectText`.
    static func of(_ entry: Entry, reactions: ReactionPick, reply: (() -> Void)?, selectText: (() -> Void)?,
                   more: [MenuItem] = []) -> MessageMenu {
        let replies = reply.map { [MenuItem("Reply", icon: "arrowshape.turn.up.left", action: $0)] } ?? []
        return MessageMenu(entry: entry, reactions: reactions, actions: replies + more,
                           selectText: offersSelectText(entry, replyable: reply != nil) ? selectText : nil)
    }

    /// Select Text goes where Reply does, on a message with text of its own (an artifact card's is only its title).
    static func offersSelectText(_ entry: Entry, replyable: Bool) -> Bool {
        replyable && entry.data.artifact == nil
            && !(entry.data.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The menu inside the picture's viewer: what leaves the viewer (Reply, Show what it did) closes it first, and
    /// the reactions are the message's current ones.
    func inViewer(reactions: ReactionPick, close: @escaping () -> Void) -> MessageMenu {
        let actions = actions.map { item in
            var item = item
            let run = item.action
            item.action = { close(); run() }
            return item
        }
        return MessageMenu(entry: entry, reactions: reactions, actions: actions)
    }
}
