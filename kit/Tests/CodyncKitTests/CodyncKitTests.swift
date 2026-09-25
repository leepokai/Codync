import Foundation
import Testing
@testable import CodyncKit

@Test func parsesHostEvents() throws {
    let url = try #require(Bundle.module.url(forResource: "events", withExtension: "txt", subdirectory: "Fixtures"))
    let lines = try String(contentsOf: url, encoding: .utf8).split(separator: "\n")
    let events = lines.compactMap { HostClient.parseEvent(Data($0.dropFirst(5).utf8)) }
    #expect(events.count == 6)
    guard case let .hello(hostId, rev, usage) = events[0] else { Issue.record("hello"); return }
    #expect(hostId == "h1" && rev == 42 && usage.providers.first?.windows.first?.percent == 8)
    guard case let .bot(bot) = events[1] else { Issue.record("bot"); return }
    #expect(bot.name == "Rex" && bot.command == nil && !bot.isWorking)
    guard case let .entry(perm) = events[2] else { Issue.record("perm"); return }
    #expect(perm.isChat && perm.data.options?.first?.kind == "allow_once")
    guard case let .entry(tool) = events[3] else { Issue.record("tool"); return }
    #expect(!tool.isChat && tool.data.diffs?.first?.added == 1)
    guard case let .botDeleted(id, _) = events[4] else { Issue.record("deleted"); return }
    #expect(id == "b2")
}

@Test func parsesPairingURL() throws {
    let p = try #require(Pairing(string: "codync://pair?name=Kevin%27s%20Mac&token=t0k&urls=http%3A%2F%2F100.1.2.3%3A19222%2Chttp%3A%2F%2Fa%3A1"))
    #expect(p.name == "Kevin's Mac")
    #expect(p.token == "t0k")
    #expect(p.urls == ["http://100.1.2.3:19222", "http://a:1"])
    #expect(Pairing(string: "codync://pair?token=") == nil)
}

@Test func relativeTime() {
    let now = Date(timeIntervalSince1970: 1_000_000)
    #expect(RelativeTime.short(now.addingTimeInterval(-30), now: now) == "now")
    #expect(RelativeTime.short(now.addingTimeInterval(-600), now: now) == "10m")
    #expect(RelativeTime.short(now.addingTimeInterval(-7200), now: now) == "2h")
}
