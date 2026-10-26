import Foundation
import Testing
@testable import CodyncKit

struct RelayCopyTests {
    @Test func officialCloudsKeepTheCloudflareName() {
        #expect(RelayCopy.host(of: URL(string: "https://api.codync.dev")) == nil)
        #expect(RelayCopy.host(of: URL(string: "https://dev-api.codync.dev/")) == nil)
        #expect(RelayCopy.host(of: nil) == nil)
        #expect(RelayCopy.connected(cloud: URL(string: "https://api.codync.dev")) == "Connected through Cloudflare")
        #expect(RelayCopy.routeTitle(.automatic, cloud: nil) == "Auto: Wi-Fi or Tailscale, then Cloudflare")
        #expect(RelayCopy.routeShortTitle(.cloudflareFirst, cloud: nil) == "Cloudflare first")
    }

    @Test func anotherHostIsNamedInTheSentence() {
        let cloud = URL(string: "https://relay.example.com")
        #expect(RelayCopy.host(of: cloud) == "relay.example.com")
        #expect(RelayCopy.host(of: URL(string: "http://127.0.0.1:8788")) == "127.0.0.1:8788")
        #expect(RelayCopy.host(of: URL(string: "https://[2001:db8::1]:8443")) == "[2001:db8::1]:8443")
        #expect(RelayCopy.host(of: URL(string: "https://[2001:db8::1]")) == "[2001:db8::1]")
        #expect(RelayCopy.connected(cloud: cloud) == "Connected through Private relay (relay.example.com)")
        #expect(RelayCopy.through(cloud: cloud) == "Through Private relay (relay.example.com)")
        #expect(RelayCopy.routeTitle(.cloudflareFirst, cloud: cloud) == "Private relay (relay.example.com), then Wi-Fi or Tailscale")
        #expect(RelayCopy.routeShortTitle(.cloudflareFirst, cloud: cloud) == "Private relay first")
        #expect(RelayCopy.detailSuffix(route: .relay, cloud: cloud) == " · Private relay (relay.example.com)")
        #expect(RelayCopy.pairing(cloud: cloud).contains("Private relay (relay.example.com)"))
    }

    @Test func aPastedLinkNamesItsRelay() {
        let v = Vectors.shared
        var comps = URLComponents(string: "codync://pair")!
        comps.queryItems = [
            URLQueryItem(name: "v", value: "3"),
            URLQueryItem(name: "id", value: v.str("keys", "computerId")),
            URLQueryItem(name: "sk", value: v.str("keys", "hostSignPub")),
            URLQueryItem(name: "bk", value: v.str("keys", "hostBoxPub")),
            URLQueryItem(name: "code", value: v.str("pairing", "code")),
            URLQueryItem(name: "cloud", value: "https://relay.example.com"),
        ]
        #expect(RelayCopy.pairing(pasted: comps.string!).contains("Private relay (relay.example.com)"))
        #expect(RelayCopy.pairing(pasted: "not a link").contains("through Cloudflare"))
    }
}
