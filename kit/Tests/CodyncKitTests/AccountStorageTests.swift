import Foundation
import Testing
@testable import CodyncKit

@Test func accountPairingsStayIsolatedIncludingLateWrites() throws {
    let suite = "CodyncAccountTests.\(UUID().uuidString)"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defer { defaults.removePersistentDomain(forName: suite) }
    let alice = SharedStore.Context(accountID: "user_alice", defaults: defaults)
    let bob = SharedStore.Context(accountID: "user_bob", defaults: defaults)
    let a = Pairing(name: "Same name", token: "alice-token", urls: ["https://alice.invalid"])
    let b = Pairing(name: "Same name", token: "bob-token", urls: ["https://bob.invalid"])
    alice.pairing = a
    alice.computers = [a]
    bob.pairing = b
    bob.computers = [b]

    // A retained store finishes work after the UI has selected Bob.
    alice.preferredURL = a.urls.first
    alice.usage = Usage()
    #expect(bob.pairing == b)
    #expect(bob.computers == [b])
    #expect(bob.preferredURL == nil)
    #expect(bob.usage == nil)
    #expect(alice.orderedPairing == a)
    #expect(alice.id != bob.id)
    #expect(SharedStore.Context(accountID: "user_alice", defaults: defaults).pairing == a)

    let oldWidget = alice.botURL("shared-bot-id")
    #expect(alice.acceptsBotURL(oldWidget))
    #expect(!bob.acceptsBotURL(oldWidget))
    #expect(!bob.acceptsBotURL(URL(string: "codync://bot/shared-bot-id")!))
    alice.pairing = b
    #expect(!alice.acceptsBotURL(oldWidget))
}

@Test func signingInDoesNotClaimLegacyLocalPairings() throws {
    let suite = "CodyncAccountTests.\(UUID().uuidString)"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defer { defaults.removePersistentDomain(forName: suite) }
    let old = Pairing(name: "Legacy Mac", token: "local-token", urls: ["http://127.0.0.1:19222"])
    defaults.set(try JSONEncoder().encode(old), forKey: "pairing")
    let local = SharedStore.Context(accountID: nil, defaults: defaults)
    let signedIn = SharedStore.Context(accountID: "user_new", defaults: defaults)

    #expect(local.pairing == old)
    #expect(local.computers == [old])
    #expect(signedIn.pairing == nil)
    #expect(signedIn.computers.isEmpty)
    signedIn.pairing = old
    signedIn.pairing = nil
    #expect(local.pairing == old)
}

@Test func widgetCachesFollowTheSelectedComputer() throws {
    let suite = "CodyncWidgetTests.\(UUID().uuidString)"
    let defaults = try #require(UserDefaults(suiteName: suite))
    defer { defaults.removePersistentDomain(forName: suite) }
    let account = SharedStore.Context(accountID: "user_a", defaults: defaults)
    let other = SharedStore.Context(accountID: "user_b", defaults: defaults)
    let first = Pairing(name: "Mac", token: "mac-token", urls: ["https://mac.invalid"])
    let second = Pairing(name: "Linux", token: "linux-token", urls: ["https://linux.invalid"])
    account.pairing = first
    account.usage = .widgetPreview
    account.bots = Bot.widgetPreview
    account.preferredURL = first.urls.first
    other.pairing = first
    other.usage = .widgetPreview
    other.bots = Bot.widgetPreview

    // Refreshing the addresses of the same computer preserves its snapshot.
    var updated = first
    updated.urls.append("https://mac-backup.invalid")
    account.pairing = updated
    #expect(account.usage == .widgetPreview)
    #expect(account.bots == Bot.widgetPreview)

    account.pairing = second
    #expect(account.usage == nil)
    #expect(account.bots.isEmpty)
    #expect(account.preferredURL == nil)
    #expect(other.usage == .widgetPreview)
    #expect(other.bots == Bot.widgetPreview)

    account.usage = .widgetPreview
    account.bots = Bot.widgetPreview
    account.pairing = nil
    #expect(account.usage == nil)
    #expect(account.bots.isEmpty)
    #expect(other.pairing == first)
}
