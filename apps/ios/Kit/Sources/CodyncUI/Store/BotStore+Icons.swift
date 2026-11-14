import CodyncKit
import Foundation
import os

extension BotStore {
    /// Stores a bot the host sent, with its custom icon.
    @discardableResult
    func putHostBot(_ bot: Bot) -> Bot {
        let bot = withIcon(bot)
        bots[bot.id] = bot
        return bot
    }

    /// Icons sync apart from bots, so no bot the host sends carries its custom icon.
    private func withIcon(_ bot: Bot) -> Bot {
        var bot = bot
        bot.customIcon = icons["bot:\(bot.id)"]
        return bot
    }

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
        for (id, bot) in bots {
            let next = withIcon(bot)
            guard next.customIcon != bot.customIcon else { continue }
            bots[id] = next
            onBotUpdated?(next)
        }
        onRosterChanged?()
    }
}
