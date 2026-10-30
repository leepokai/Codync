import CodyncKit
import Foundation
import os

extension BotStore {
    func applyIcons(_ updated: [String: CustomIcon], revision: Int64 = 0) {
        guard revision >= iconsRevision else { return }
        iconsRevision = revision
        guard icons != updated else { return }
        for target in Set(icons.keys).union(updated.keys) where target.hasPrefix("bot:") && icons[target] != updated[target] {
            do {
                try ActivityIconCache.writeReference(updated[target], computerId: computer.id, botId: String(target.dropFirst(4)), context: storage.id)
            } catch { Logger(subsystem: "com.pokai.Codync", category: "CustomIcon").error("Cannot cache activity icon: \(error.localizedDescription)") }
        }
        do { try ActivityIconCache.prune(context: storage.id) }
        catch { Logger(subsystem: "com.pokai.Codync", category: "CustomIcon").error("Cannot prune activity icons: \(error.localizedDescription)") }
        icons = updated
        for (id, var bot) in bots {
            guard bot.customIcon != updated["bot:\(id)"] else { continue }
            bot.customIcon = updated["bot:\(id)"]
            bots[id] = bot
            onBotUpdated?(bot)
        }
        onRosterChanged?()
    }
}
