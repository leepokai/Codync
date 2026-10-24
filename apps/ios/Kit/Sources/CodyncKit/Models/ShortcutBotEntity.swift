import AppIntents
import Foundation

/// Shortcuts keep the account scope and computer in the ID, just like widget links.
public struct ShortcutBotEntity: AppEntity {
    public static let typeDisplayRepresentation: TypeDisplayRepresentation = "Bot"
    public static let defaultQuery = ShortcutBotQuery()

    public let id: String
    public let name: String
    public let computerName: String

    public var displayRepresentation: DisplayRepresentation {
        DisplayRepresentation(title: "\(name)", subtitle: "\(computerName)")
    }

    public func reference(in storage: SharedStore.Context) -> BotReference? {
        guard let url = URL(string: id), let ref = storage.reference(from: url),
              storage.computers.contains(where: { $0.id == ref.computerId }),
              storage.bots.contains(where: { $0.computerId == ref.computerId && $0.bot.id == ref.botId && !$0.bot.hidden }) else { return nil }
        return ref
    }
}

public struct ShortcutBotQuery: EntityStringQuery {
    public init() {}

    public func entities(for identifiers: [String]) async throws -> [ShortcutBotEntity] {
        let all = Self.bots(in: SharedStore.activeContext)
        return identifiers.compactMap { id in all.first { $0.id == id } }
    }

    public func entities(matching string: String) async throws -> [ShortcutBotEntity] {
        Self.bots(in: SharedStore.activeContext).filter { $0.name.localizedStandardContains(string) }
    }

    public func suggestedEntities() async throws -> [ShortcutBotEntity] {
        Self.bots(in: SharedStore.activeContext)
    }

    static func bots(in storage: SharedStore.Context) -> [ShortcutBotEntity] {
        let computers = storage.computers
        return storage.bots.compactMap { snapshot in
            guard !snapshot.bot.hidden, let computer = computers.first(where: { $0.id == snapshot.computerId }) else { return nil }
            let ref = BotReference(accountId: storage.accountID, computerId: computer.id, botId: snapshot.bot.id)
            return ShortcutBotEntity(id: storage.botURL(ref).absoluteString, name: snapshot.bot.name, computerName: computer.name)
        }.sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
    }
}
