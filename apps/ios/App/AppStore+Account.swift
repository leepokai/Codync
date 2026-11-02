import CodyncKit
import CodyncUI

/// Leaving an account (signing out, deleting it) and moving signed-out pairings into one.
extension AppStore {
    /// Sign-in is required: computers paired while signed out move into the account the first time one is
    /// used here, before its stores and keys load. Builds without Clerk stay in the signed-out context.
    static func adoptSignedOutPairings(into storage: SharedStore.Context) {
        guard let adoption = storage.adopt(from: SharedStore.Context(accountID: nil)), adoption.computers > 0 else { return }
        Analytics.shared.capture(.signedOutPairingsMoved(computers: adoption.computers, keys: adoption.keys))
    }

    /// Signing out: Clerk first, so a sign-out that fails keeps the key. Then this iPhone's key goes back (the
    /// computers it reaches and the account drop it) and the account is forgotten here (spec §3.2).
    func signOut() async {
        guard let userID = account.userID else { return }
        // Sent first so it still counts toward the account.
        Analytics.shared.capture(.signedOut)
        // The cloud call goes out after Clerk signed out, on a token taken now (it outlives the session).
        let cloud = await cloud?.pinningToken()
        leaving = true
        await account.signOut()
        if hasLeft(userID) { await handBack(cloud: cloud) }
        forget(userID)
    }

    /// Delete account: the user goes in Clerk first, and nothing changes when that fails. Then this iPhone leaves the
    /// computers it reaches and forgets the account, as signing out does. The cloud removes the account's devices,
    /// grants and computers itself, on Clerk's `user.deleted`.
    func deleteAccount() async {
        guard let userID = account.userID else { return }
        leaving = true
        do {
            try await account.deleteUser()
            await handBack(cloud: nil)
        } catch {
            account.errorMessage = "Couldn't delete your account. Check your connection and try again."
        }
        forget(userID)
    }

    /// After leaving `userID`: the switch held off while `leaving` runs (a retiring store saves its cache one last
    /// time), then its device keys, computers and caches go, unless it's still signed in here.
    private func forget(_ userID: String) {
        leaving = false
        switchAccount(to: account.userID)
        guard hasLeft(userID) else { return }
        SharedStore.Context(accountID: userID).erase()
        // The last account gone: the sign-in screen takes over.
        if !account.isSignedIn { account.showSwitcher = false }
    }

    private func hasLeft(_ userID: String) -> Bool {
        account.userID != userID && !account.accounts.contains(where: { $0.id == userID })
    }

    /// This iPhone's key goes back: each computer it's connected to removes it (older hosts only stop its alerts),
    /// and with `cloud` the account revokes it. Best effort, a few seconds at most: leaving never waits on an
    /// unreachable computer or cloud, and the key is erased right after.
    func handBack(cloud: CloudClient?) async {
        let clients = accounts.computers.compactMap { accounts.store(for: $0.id) }
            .filter { $0.connection == .online }.compactMap(\.client)
        await withTaskGroup(of: Void.self) { group in
            for client in clients { group.addTask { await Self.bestEffort(.seconds(3)) { try await client.forgetDevice() } } }
            if let cloud { group.addTask { await Self.bestEffort(.seconds(5)) { try await cloud.revokeThisDevice() } } }
        }
    }

    private nonisolated static func bestEffort(_ limit: Duration, _ work: @escaping @Sendable () async throws -> Void) async {
        await withTaskGroup(of: Void.self) { group in
            group.addTask { try? await work() }
            group.addTask { try? await Task.sleep(for: limit) }
            await group.next()
            group.cancelAll()
        }
    }
}
