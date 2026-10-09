import CodyncKit

/// Saves ordering on this device only, within the current account's local context.
@MainActor
final class ComputerOrderStore {
    private(set) var state: ComputerOrder
    var onChanged: (() -> Void)?
    private let storage: SharedStore.Context
    private var retired = false

    init(storage: SharedStore.Context) {
        self.storage = storage
        state = storage.computerOrder ?? ComputerOrder(ids: storage.computers.map(\.id))
    }

    func move(_ id: ComputerID, to target: ComputerID, available: [ComputerID]) {
        guard !retired, let ids = state.moving(id, to: target, available: available) else { return }
        let next = ComputerOrder(ids: ids)
        guard state != next else { return }
        state = next
        storage.computerOrder = next
        onChanged?()
    }

    func retire() {
        retired = true
        onChanged = nil
    }
}
