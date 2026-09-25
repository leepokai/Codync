import Foundation
import Testing
@testable import CodyncKit

private let v = Vectors.shared

private func link(_ overrides: [String: String?] = [:]) -> String {
    var q: [String: String?] = [
        "v": "3", "name": "Kevin's Mac", "id": v.str("keys", "computerId"), "sk": v.str("keys", "hostSignPub"),
        "bk": v.str("keys", "hostBoxPub"), "code": v.str("pairing", "code"),
        "urls": "http://192.168.1.5:19222,http://100.64.1.2:19222", "cloud": "https://codync-cloud-staging.example.workers.dev",
    ]
    for (k, value) in overrides { q[k] = value }
    var comps = URLComponents(string: "codync://pair")!
    comps.queryItems = q.keys.sorted().compactMap { k in q[k]!.map { URLQueryItem(name: k, value: $0) } }
    return comps.string!
}

@Test func parsesPairingV3() throws {
    let p = try Pairing.parse(link())
    #expect(p.computer.id == v.str("keys", "computerId"))
    #expect(p.computer.name == "Kevin's Mac")
    #expect(p.computer.signKey == v.str("keys", "hostSignPub"))
    #expect(p.computer.boxKey == v.str("keys", "hostBoxPub"))
    #expect(p.computer.urls == ["http://192.168.1.5:19222", "http://100.64.1.2:19222"])
    #expect(p.computer.cloud?.absoluteString == "https://codync-cloud-staging.example.workers.dev")
    #expect(p.code == v.str("pairing", "code"))
    #expect(p.offerId == v.str("pairing", "offerId"))
    #expect(Pairing(string: link()) == p)
    // Either route alone is enough.
    #expect(Pairing(string: link(["urls": nil])) != nil)
    #expect(Pairing(string: link(["cloud": nil])) != nil)
}

@Test func rejectsBadPairingLinks() {
    #expect(throws: Pairing.Problem.outdatedHost) { try Pairing.parse(link(["v": nil])) }
    #expect(throws: Pairing.Problem.outdatedHost) { try Pairing.parse(link(["v": "2"])) }
    #expect(throws: Pairing.Problem.invalid) { try Pairing.parse(link(["id": "AAAAAAAAAAAAAAAAAAAAAA"])) }
    #expect(throws: Pairing.Problem.invalid) { try Pairing.parse(link(["code": nil])) }
    #expect(throws: Pairing.Problem.invalid) { try Pairing.parse(link(["code": "short"])) }
    #expect(throws: Pairing.Problem.invalid) { try Pairing.parse(link(["bk": nil])) }
    #expect(throws: Pairing.Problem.invalid) { try Pairing.parse(link(["cloud": "http://evil.example.com"])) }
    #expect(throws: Pairing.Problem.invalid) { try Pairing.parse(link(["urls": nil, "cloud": nil])) }
    #expect(throws: Pairing.Problem.notPairingLink) { try Pairing.parse("https://example.com") }
    // The old token-based link is not accepted.
    #expect(throws: Pairing.Problem.outdatedHost) { try Pairing.parse("codync://pair?name=Mac&token=t0k&urls=http%3A%2F%2Fa%3A1") }
}

@Test func botLinksStayInTheirContext() throws {
    let suite = "CodyncTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let alice = SharedStore.Context(accountID: "user_alice", suite: suite)
    let bob = SharedStore.Context(accountID: "user_bob", suite: suite)
    let local = SharedStore.Context(accountID: nil, suite: suite)
    let ref = BotReference(accountId: "user_alice", computerId: v.str("keys", "computerId"), botId: "b1")

    let url = alice.botURL(ref)
    #expect(url.absoluteString == "codync://bot/b1?scope=\(alice.id)&computer=\(ref.computerId)")
    #expect(alice.reference(from: url) == ref)
    #expect(bob.reference(from: url) == nil)
    #expect(local.reference(from: url) == nil)
    #expect(alice.reference(from: URL(string: "codync://bot/b1")!) == nil)
    #expect(alice.reference(from: URL(string: "codync://bot/b1?scope=\(alice.id)&computer=nope")!) == nil)
    let localRef = BotReference(accountId: nil, computerId: ref.computerId, botId: "b1")
    #expect(local.reference(from: local.botURL(localRef)) == localRef)
}

@Test func contextsAreIsolatedAndErasable() throws {
    let suite = "CodyncTests.\(UUID().uuidString)"
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    let alice = SharedStore.Context(accountID: "user_alice", suite: suite)
    let bob = SharedStore.Context(accountID: "user_bob", suite: suite)
    alice.computers = [v.computer]
    alice.usage = [v.computer.id: Usage()]
    alice.lastComputerId = v.computer.id
    #expect(bob.computers.isEmpty && bob.usage.isEmpty && bob.lastComputerId == nil)
    #expect(SharedStore.Context(accountID: "user_alice", suite: suite).computers == [v.computer])
    alice.erase()
    #expect(alice.computers.isEmpty && alice.usage.isEmpty && alice.lastComputerId == nil)
}
