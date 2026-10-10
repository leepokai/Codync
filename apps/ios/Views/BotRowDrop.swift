import CodyncKit
import CodyncUI
import SwiftUI

/// A bot row others can be dragged onto, to reorder a computer's bots. It's a system drag,
/// so touch and hold still opens the row's menu first. The store moves the dragged bot into
/// each row it passes; the drop sends the order.
struct BotRowDrop: DropDelegate {
    let target: RosterItem
    let store: BotStore
    @Binding var dragging: BotReference?
    let reduceMotion: Bool

    func validateDrop(info: DropInfo) -> Bool {
        dragging?.computerId == target.ref.computerId
    }

    func dropEntered(info: DropInfo) {
        guard let dragging, dragging.computerId == target.ref.computerId else { return }
        withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { _ = store.move(dragging.botId, onto: target.bot.id) }
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        store.saveOrder()
        dragging = nil
        return true
    }
}
