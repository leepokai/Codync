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
    /// sections included, then each of those computers' box of threads without a bot: whose work the Threads tab lists.
    func threadRoster(hidden: String, sectionOrder: RosterSectionOrder) -> [RosterItem] {
        let shown = ComputerSelection(all: computers.map(\.id), hidden: hidden).shown
        let computers = computers.filter { shown.contains($0.id) }
        let boxes = computers.compactMap { computer -> RosterItem? in
            guard let store = store(for: computer.id), let box = store.plainBox else { return nil }
            return RosterItem(ref: BotReference(accountId: accountId, computerId: computer.id, botId: box.id), bot: box, computer: store.computer)
        }
        let older = Dictionary(uniqueKeysWithValues: computers.compactMap { c in store(for: c.id).map { (c.id, $0.olderThreads.byBot) } })
        return (computers.flatMap { onScreen($0.id, order: sectionOrder) } + boxes).map { item in
            // Older threads (`olderThreads`) join their bot's work as ended ones.
            guard let tasks = older[item.ref.computerId]?[item.bot.id] else { return item }
            var item = item
            item.bot.work = WorkTask.merging(item.bot.work, older: tasks)
            return item
        }
    }

    /// `botId` of `computer` as a roster row, hidden ones (the box of threads without a bot) included.
    func item(_ computer: ComputerID, botId: String) -> RosterItem? {
        guard let store = store(for: computer), let bot = store.bots[botId] else { return nil }
        return RosterItem(ref: BotReference(accountId: accountId, computerId: computer, botId: botId), bot: bot, computer: store.computer)
    }
}
