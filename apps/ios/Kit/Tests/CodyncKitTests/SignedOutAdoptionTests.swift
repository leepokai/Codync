import Foundation
import Testing
@testable import CodyncKit

/// The signed-out context's Keychain items are shared by every test here, so they run one at a time.
@Suite(.serialized) struct SignedOutAdoptionTests {
    private let v = Vectors.shared

    private func contexts() -> (local: SharedStore.Context, account: SharedStore.Context, suite: String) {
        Keychain.memory.withLock { $0 = $0 ?? [:] }
        let suite = "CodyncTests.\(UUID().uuidString)"
        let local = SharedStore.Context(accountID: nil, suite: suite)
        local.erase()
        return (local, SharedStore.Context(accountID: "user_\(UUID().uuidString)", suite: suite), suite)
    }

    @Test func aNewAccountContextTakesTheKeysAndComputers() throws {
        let (local, account, suite) = contexts()
        defer { account.erase(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let key = try DeviceIdentity.load(context: local)
        local.computers = [v.computer]
        local.composerDrafts = [v.computer.id: ["b1": "draft"]]
        let cache = URL.cachesDirectory.appending(path: "codync-mirror-tests-\(local.id)-\(v.computer.id).json")
        try Data("unsent".utf8).write(to: cache)
        let movedCache = URL.cachesDirectory.appending(path: "codync-mirror-tests-\(account.id)-\(v.computer.id).json")
        try Data("stale".utf8).write(to: movedCache)

        #expect(account.adopt(from: local) == .init(computers: 1, keys: .moved))
        #expect(account.computers == [v.computer])
        #expect(account.composerDrafts[v.computer.id]?["b1"] == "draft")
        #expect(try Data(contentsOf: movedCache) == Data("unsent".utf8), "unsent messages come along, over a stale cache")
        #expect(!FileManager.default.fileExists(atPath: cache.path()))
        let moved = try DeviceIdentity.load(context: account)
        #expect(moved.publicKey == key.publicKey, "hosts see the same device")
        #expect(moved.pushKey == key.pushKey, "push keeps opening")
        #expect(local.computers.isEmpty && !DeviceIdentity.exists(context: local))
        #expect(account.adopt(from: local) == nil, "nothing left to move")
    }

    @Test func aUsedAccountContextKeepsItsKeyAndListsTheComputers() throws {
        let (local, account, suite) = contexts()
        defer { account.erase(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let own = try DeviceIdentity.load(context: account)
        var mine = v.computer
        mine.name = "Account copy"
        account.computers = [mine]
        _ = try DeviceIdentity.load(context: local)
        var other = v.computer
        other.id = "other"
        local.computers = [v.computer, other]

        #expect(account.adopt(from: local) == .init(computers: 2, keys: .rescan))
        #expect(account.computers.map(\.id) == [v.computer.id, "other"])
        #expect(account.computers.first?.name == "Account copy", "the account's entry wins")
        #expect(try DeviceIdentity.load(context: account).publicKey == own.publicKey)
        #expect(local.computers.isEmpty && !DeviceIdentity.exists(context: local))
    }

    @Test func aUsedAccountContextTakesTheChatsOfTheComputersItGains() throws {
        let (local, account, suite) = contexts()
        defer { account.erase(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        _ = try DeviceIdentity.load(context: account)
        account.computers = [v.computer]
        _ = try DeviceIdentity.load(context: local)
        var other = v.computer
        other.id = "other"
        local.computers = [v.computer, other]
        func cache(_ context: SharedStore.Context, _ id: String) -> URL {
            URL.cachesDirectory.appending(path: "codync-mirror-tests-\(context.id)-\(id).json")
        }
        try Data("waiting".utf8).write(to: cache(local, "other"))
        try Data("signed out".utf8).write(to: cache(local, v.computer.id))
        try Data("account".utf8).write(to: cache(account, v.computer.id))

        #expect(account.adopt(from: local)?.keys == .rescan)
        #expect(try Data(contentsOf: cache(account, "other")) == Data("waiting".utf8), "waiting messages aren't lost")
        #expect(try Data(contentsOf: cache(account, v.computer.id)) == Data("account".utf8), "a computer it has keeps its chat")
        #expect(!FileManager.default.fileExists(atPath: cache(local, "other").path()))
    }

    @Test func nothingToMove() {
        let (local, account, suite) = contexts()
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        #expect(account.adopt(from: local) == nil)
        #expect(local.adopt(from: account) == nil, "only into an account context")
    }

    @Test func aCacheThatCannotMoveStays() throws {
        let (local, account, suite) = contexts()
        defer { account.erase(); UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        _ = try DeviceIdentity.load(context: local)
        local.computers = [v.computer]
        let cache = URL.cachesDirectory.appending(path: "codync-mirror-tests-\(local.id)-\(v.computer.id).json")
        try Data("unsent".utf8).write(to: cache)
        defer { try? FileManager.default.removeItem(at: cache) }
        // A folder that can't be emptied sits at the account's name, so the rename fails.
        let blocked = URL.cachesDirectory.appending(path: "codync-mirror-tests-\(account.id)-\(v.computer.id).json")
        try FileManager.default.createDirectory(at: blocked, withIntermediateDirectories: true)
        try Data().write(to: blocked.appending(path: "inner"))
        try FileManager.default.setAttributes([.posixPermissions: 0o555], ofItemAtPath: blocked.path())
        defer {
            try? FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: blocked.path())
            try? FileManager.default.removeItem(at: blocked)
        }

        #expect(account.adopt(from: local) == .init(computers: 1, keys: .moved))
        #expect(try Data(contentsOf: cache) == Data("unsent".utf8), "the only copy of those messages isn't deleted")
        #expect(local.computers.isEmpty && !DeviceIdentity.exists(context: local))
    }

    @Test func startingOverErasesAccountsSignedOutHereToo() throws {
        let (local, account, suite) = contexts()
        defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
        let signedOutHere = SharedStore.Context(accountID: "user_\(UUID().uuidString)", suite: suite)
        for context in [local, account, signedOutHere] {
            _ = try DeviceIdentity.load(context: context)
            context.computers = [v.computer]
        }
        let cache = URL.cachesDirectory.appending(path: "codync-mirror-tests-\(signedOutHere.id)-\(v.computer.id).json")
        try Data("unsent".utf8).write(to: cache)

        SharedStore.eraseAll(suite: suite)

        for context in [local, account, signedOutHere] {
            #expect(context.computers.isEmpty && !DeviceIdentity.exists(context: context))
        }
        #expect(!FileManager.default.fileExists(atPath: cache.path()))
    }
}
