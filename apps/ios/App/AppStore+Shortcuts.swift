import CodyncKit
import CodyncUI
import Foundation
import UIKit

extension AppStore {
    func openShortcutLive(_ bot: ShortcutBotEntity) async throws {
        let (ref, store) = try await shortcutTarget(bot)
        show(ref)
        try await store.requestVoiceCall(to: ref.botId)
    }

    func sendShortcutText(_ text: String, to bot: ShortcutBotEntity, openChat: Bool) async throws -> SendProgress {
        let (ref, store) = try await shortcutTarget(bot)
        if openChat {
            show(ref)
        } else if UIApplication.shared.applicationState == .background {
            // Only the target's pending send retains a connection while the app is backgrounded.
            accounts.setActive(false)
        }
        return try await store.sendAndWait(text, to: ref.botId)
    }

    /// Waits for Clerk to restore the account first: an intent can run before the scene's userID observer.
    private func shortcutTarget(_ bot: ShortcutBotEntity) async throws -> (BotReference, BotStore) {
        let ready = await account.waitUntilReady(timeout: .seconds(10))
        try Task.checkCancellation()
        guard ready else { throw HostError.unreachable }
        switchAccount(to: account.userID)
        guard let ref = bot.reference(in: storage), let store = accounts.store(for: ref.computerId) else {
            throw ShortcutError.botUnavailable
        }
        return (ref, store)
    }

    private func show(_ ref: BotReference) {
        for store in accounts.stores.values { store.screenRequest = nil }
        account.showSwitcher = false
        showComputers = false
        showUsage = false
        marketplace = nil
        tab = .bots
        accounts.selection = ref
        accounts.setActive(true)
    }
}
