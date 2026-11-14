import Foundation
import Testing
@testable import CodyncKit

struct ScreenRouteTests {
    private func turn(onRelay: Bool = false, _ route: ConnectionRoute, hasTurn: Bool = true, failed: Int = 0) -> Bool {
        ScreenRoute.asksForTurn(channelOnRelay: onRelay, route: route, hasTurn: hasTurn, failedAttempts: failed)
    }

    @Test func autoGoesDirectThenThroughTurn() {
        #expect(!turn(.automatic))
        #expect(turn(.automatic, failed: 1))
        #expect(!turn(.automatic, failed: 2))
        #expect(turn(onRelay: true, .automatic), "chat came through the relay: there's no direct way to start with")
        #expect(turn(onRelay: true, .automatic, failed: 1))
    }

    @Test func cloudflareFirstGoesThroughTurnThenDirect() {
        #expect(turn(.cloudflareFirst))
        #expect(!turn(.cloudflareFirst, failed: 1))
        #expect(turn(onRelay: true, .cloudflareFirst))
    }

    @Test func directOnlyNeverAsks() {
        for failed in 0...2 { #expect(!turn(.directOnly, failed: failed)) }
    }

    @Test func aComputerInNoAccountHasNoTurn() {
        for route in ConnectionRoute.allCases {
            for failed in 0...2 { #expect(!turn(route, hasTurn: false, failed: failed)) }
        }
    }

    @Test func aPrivateRelayStillSaysItHasNoScreen() {
        // Chat over a private relay asks, and the host explains that it has no TURN.
        #expect(turn(onRelay: true, .automatic, hasTurn: false))
    }
}

struct LocalDiscoveryTests {
    @Test func keepsOnlyValidAddresses() throws {
        let entry = try #require(LocalDiscovery.entry(id: "c1", urls: "http://192.168.1.5:19222,not a url,http://10.0.0.2:19222"))
        #expect(entry.id == "c1")
        #expect(entry.urls == ["http://192.168.1.5:19222", "http://10.0.0.2:19222"])
        #expect(LocalDiscovery.entry(id: nil, urls: "http://192.168.1.5:19222") == nil)
        #expect(LocalDiscovery.entry(id: "c1", urls: nil) == nil)
        #expect(LocalDiscovery.entry(id: "c1", urls: "nope") == nil)
    }
}

struct SignedOutPairingTests {
    private func pairing(cloud: String?) -> Pairing {
        let computer = Computer(id: "c", name: "Mac", signKey: "k", urls: ["http://192.168.1.5:19222"], cloud: cloud.flatMap(URL.init(string:)))
        return Pairing(computer: computer, code: "code")
    }

    @Test func aSignedOutPhoneLeavesOutOnlyTheCodyncRelay() {
        #expect(pairing(cloud: "https://api.codync.dev").withoutRelay.computer.cloud == nil)
        let own = pairing(cloud: "https://relay.example.com").withoutRelay.computer
        #expect(own.cloud?.host() == "relay.example.com", "a private relay is the user's own")
        #expect(own.urls == ["http://192.168.1.5:19222"])
    }

    @Test func aFailedAddToAccountSaysWhy() {
        typealias Failure = AnalyticsEvent.AddToAccountFailure
        #expect(Failure(HostError.http(500, "The computer stayed out of this account.")) == .declined)
        #expect(Failure(HostError.http(500, "Couldn't confirm your Codync account. Sign in again.")) == .certificate)
        #expect(Failure(CloudError(status: 0, code: "noCertificate", message: nil)) == .certificate)
        #expect(Failure(HostError.unreachable) == .unreachable)
        #expect(Failure(URLError(.notConnectedToInternet)) == .unreachable)
        #expect(Failure(CancellationError()) == .other)
    }
}
