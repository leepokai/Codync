import SwiftUI

/// What a move does to the agents' order. The host keeps the order; this says what to send it.
public enum UsageOrder {
    /// `ids` after a drop, as `onMove` reports it; nil when the row landed where it was.
    public static func moved(_ ids: [String], from source: IndexSet, to destination: Int) -> [String]? {
        var moved = ids
        moved.move(fromOffsets: source, toOffset: destination)
        return moved == ids ? nil : moved
    }
}
