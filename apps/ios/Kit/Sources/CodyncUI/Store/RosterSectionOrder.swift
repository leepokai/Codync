import CodyncKit
import Foundation

/// This device's section order per computer. The empty name stands for the unsectioned Bots group.
public struct RosterSectionOrder: Codable, Sendable {
    private var computers: [ComputerID: [String]] = [:]

    public init(data: Data) {
        self = (try? JSONDecoder().decode(Self.self, from: data)) ?? Self()
    }

    private init() {}

    public var data: Data { (try? JSONEncoder().encode(self)) ?? Data() }

    public func saved(on computer: ComputerID) -> [String] { computers[computer] ?? [] }

    public mutating func set(_ names: [String], on computer: ComputerID) {
        computers[computer] = names
    }

    public mutating func rename(_ from: String, to: String?, on computer: ComputerID) {
        guard from != to else { return }
        let order = computers[computer] ?? []
        computers[computer] = order.compactMap { name in
            guard name == from else { return name }
            return to.flatMap { order.contains($0) ? nil : $0 }
        }
    }
}

public extension AccountStore {
    /// `computer`'s bots in the order the Bots tab has them (`RosterSection.onScreen`), the ones named like `query` when
    /// there is one.
    func onScreen(_ computer: ComputerID, order: RosterSectionOrder, matching query: String = "") -> [RosterItem] {
        RosterSection.onScreen(roster.filter { $0.ref.computerId == computer }, order: order.saved(on: computer), matching: query) { $0.bot }
    }

    /// Every bot on the computers the Bots tab shows (`hidden`: the ones left out), in its order, folded computers and
    /// sections included, with how much each one works on: the Threads tab.
    func work(hidden: String, sectionOrder: RosterSectionOrder) -> AllWork<RosterItem> {
        let shown = ComputerSelection(all: computers.map(\.id), hidden: hidden).shown
        let rows = computers.filter { shown.contains($0.id) }.flatMap { onScreen($0.id, order: sectionOrder) }
        return AllWork(rows) { $0.bot.work }
    }
}
