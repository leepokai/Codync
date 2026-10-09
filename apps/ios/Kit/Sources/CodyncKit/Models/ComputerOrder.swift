import Foundation

/// Device-local order, including computers temporarily unavailable in this account.
public struct ComputerOrder: Codable, Equatable, Sendable {
    public var ids: [ComputerID]

    public init(ids: [ComputerID] = []) {
        self.ids = ids
    }

    public func orderedIDs(_ available: [ComputerID]) -> [ComputerID] {
        let known = Set(available)
        var seen: Set<ComputerID> = []
        return (ids + available).filter { known.contains($0) && seen.insert($0).inserted }
    }

    public func moving(_ id: ComputerID, to target: ComputerID, available: [ComputerID]) -> [ComputerID]? {
        var seen: Set<ComputerID> = []
        var next = (ids + available).filter { seen.insert($0).inserted }
        guard id != target, let from = next.firstIndex(of: id), let to = next.firstIndex(of: target) else { return nil }
        next.insert(next.remove(at: from), at: to)
        return next
    }
}
