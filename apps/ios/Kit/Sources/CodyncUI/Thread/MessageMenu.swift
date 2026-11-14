import CodyncKit
import SwiftUI

/// A message's long-press menu: its quick reactions, then Copy, then the rest (Reply, Show what it did…). A picture
/// in the message, in the chat or its viewer, shows the same menu with Copy image and Copy name in Copy's place.
struct MessageMenu {
    let entry: Entry
    let reactions: ReactionPick
    /// What follows Copy.
    let actions: [MenuItem]

    /// The plain Copy: the message's text.
    var textCopy: [MenuItem] {
        [MenuItem("Copy", icon: "square.on.square") { Pasteboard.copy(entry.data.text) }]
    }

    func items(copy: [MenuItem]) -> [MenuItem] { copy + actions }

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
