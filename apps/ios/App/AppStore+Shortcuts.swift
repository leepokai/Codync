import CodyncKit
import CodyncUI
import Foundation
import UIKit

extension AppStore {
    func openShortcutBot(_ bot: ShortcutBotEntity) throws -> BotStore {
        let (ref, store) = try shortcutTarget(bot)
        for store in accounts.stores.values { store.screenRequest = nil }
        account.showSwitcher = false
        showComputers = false
        showUsage = false
        marketplace = nil
        tab = .bots
        accounts.selection = ref
        accounts.setActive(true)
        return store
    }

    func sendShortcutText(_ text: String, to bot: ShortcutBotEntity, openChat: Bool) async throws -> SendProgress {
        // A background launch has no scene's userID observer to restore the account context.
        let deadline = ContinuousClock.now + .seconds(10)
        while !account.isReady {
            guard ContinuousClock.now < deadline else { throw HostError.unreachable }
            try await Task.sleep(for: .milliseconds(100))
        }
        try Task.checkCancellation()
        switchAccount(to: account.userID)
        let (ref, store) = try shortcutTarget(bot)
        if openChat {
            _ = try openShortcutBot(bot)
        } else if UIApplication.shared.applicationState != .active {
            // Only the target's pending send retains a connection while the app is backgrounded.
            accounts.setActive(false)
        }
        return try await store.sendAndWait(text, to: ref.botId)
    }

    private func shortcutTarget(_ bot: ShortcutBotEntity) throws -> (BotReference, BotStore) {
        guard let ref = bot.reference(in: storage), let store = accounts.store(for: ref.computerId) else {
            throw HostError.http(404, "This bot isn't available in the current account. Open Codync and choose a bot again in the shortcut.")
        }
        return (ref, store)
    }
}
