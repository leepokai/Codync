import Foundation
import Testing
@testable import CodyncKit

@Suite struct McpAccessTests {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    @Test func listsTheAppsNotRevokedNewestFirstAndSkipsWhatItCantRead() throws {
        let ms = Int64(now.timeIntervalSince1970 * 1000)
        let json = """
        {"clients":[
          {"id":"a","label":"Laptop agent","access":"auto","createdAt":1,"expiresAt":\(ms + 3 * 86_400_000)},
          {"id":"b","label":"Old","access":"full","createdAt":2,"expiresAt":\(ms),"revokedAt":5},
          {"label":"no id"},
          {"id":"c","label":"Odd","access":"root","createdAt":3,"expiresAt":\(ms - 1),"tokenHash":"x"}
        ],"levels":{"max":"auto"}}
        """
        let apps = try JSONDecoder().decode(McpApps.self, from: Data(json.utf8))
        #expect(apps.active.map(\.id) == ["c", "a"])
        #expect(apps.active[1].daysLeft(now: now) == 3)
        #expect(apps.active[0].daysLeft(now: now) == 0, "past its expiry it has none left")
        #expect(apps.active[0].levelName == "Read only", "an unknown level reads as the least")
        #expect(try JSONDecoder().decode(McpApps.self, from: Data("{}".utf8)).clients.isEmpty)
    }

    @Test func helloSaysWhetherPhonesMayListThem() throws {
        let hello = #"{"hostId":"h1","name":"Mac","version":"3.0.0","os":"macos","backends":[],"rev":0"#
        #expect(try JSONDecoder().decode(Hello.self, from: Data((hello + #","mcpAccess":true}"#).utf8)).mcpAccess == true)
        #expect(try JSONDecoder().decode(Hello.self, from: Data((hello + "}").utf8)).mcpAccess == nil)
    }
}
