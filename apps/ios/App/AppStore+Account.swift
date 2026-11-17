import CodyncKit
import CodyncUI

/// Leaving an account (signing out, deleting it) and moving signed-out pairings into one.
extension AppStore {
    /// Computers paired while signed out move into the account the first time one is used here, before its stores and
    /// keys load. Builds without Clerk stay in the signed-out context. True when they have to be paired again: this
    /// account has its own key on this iPhone.
    @discardableResult
    static func adoptSignedOutPairings(into storage: SharedStore.Context) -> Bool {
        guard let adoption = storage.adopt(from: SharedStore.Context(accountID: nil)), adoption.computers > 0 else { return false }
        Analytics.shared.capture(.signedOutPairingsMoved(computers: adoption.computers, keys: adoption.keys))
        return adoption.keys == .rescan
    }

    static let scanAgain = "This account already has its own key on this iPhone. Scan the code of each computer you paired without an account again; your messages are kept."

    /// Signing out: Clerk first, so a sign-out that fails changes nothing. This iPhone keeps the account's key,
    /// computers and drafts, so signing back in needs no scan or check; only its alerts stop: the computers it reaches
    /// drop them now, the others when the account tells them (connections.md, accounts on a phone).
    func signOut() async {
        guard let userID = account.userID else { return }
        // Sent first so it still counts toward the account.
        Analytics.shared.capture(.signedOut)
        // The cloud call goes out after Clerk signed out, on a token taken now (it outlives the session).
        let cloud = await cloud?.pinningToken()
        leaving = true
        await account.signOut()
        if hasLeft(userID) {
            // No push ticket goes out from here: a computer keeps one that reaches it after the account marks this
            // iPhone signed out, taking it for one sent on signing back in.
            accounts.onConnected = nil
            PushRegistrar.shared.deactivate()
            await tell(computers: { try await $0.unregisterDevice() }, cloud: cloud, account: { try await $0.signOutThisDevice() })
        }
        finishLeaving()
    }

    /// Delete account: the user goes in Clerk first, and nothing changes when that fails. Then this iPhone leaves the
    /// computers it reaches and forgets the account. The cloud removes the account's devices,
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

    /// After deleting `userID`: as `finishLeaving()`, then its device keys, computers and caches go, unless it's
    /// still signed in here.
    private func forget(_ userID: String) {
        finishLeaving()
        if hasLeft(userID) { SharedStore.Context(accountID: userID).erase() }
    }

    /// The switch held off while `leaving` runs (a retiring store saves its cache one last time); the last account
    /// gone, the sign-in screen takes over.
    private func finishLeaving() {
        leaving = false
        switchAccount(to: account.userID)
        if !account.isSignedIn { account.showSwitcher = false }
    }

    private func hasLeft(_ userID: String) -> Bool {
        account.userID != userID && !account.accounts.contains(where: { $0.id == userID })
    }

    /// This iPhone's key goes back (deleting the account, starting over): each computer it's connected to removes it
    /// (older hosts only stop its alerts), and with `cloud` the account revokes it. The key is erased right after.
    func handBack(cloud: CloudClient?) async {
        await tell(computers: { try await $0.forgetDevice() }, cloud: cloud, account: { try await $0.revokeThisDevice() })
    }

    /// `computers` on each computer this iPhone is connected to, and `account` on `cloud`, at once. Best effort, a
    /// few seconds at most: leaving never waits on an unreachable computer or cloud.
    private func tell(computers: @escaping @Sendable (HostClient) async throws -> Void, cloud: CloudClient?,
                      account: @escaping @Sendable (CloudClient) async throws -> Void) async {
        let clients = accounts.computers.compactMap { accounts.store(for: $0.id) }
            .filter { $0.connection == .online }.compactMap(\.client)
        await withTaskGroup(of: Void.self) { group in
            for client in clients { group.addTask { await Self.bestEffort(.seconds(3)) { try await computers(client) } } }
            if let cloud { group.addTask { await Self.bestEffort(.seconds(5)) { try await account(cloud) } } }
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
