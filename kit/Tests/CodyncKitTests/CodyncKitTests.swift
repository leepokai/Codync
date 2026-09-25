import Foundation
import Testing
@testable import CodyncKit

@Test func parsesHostEvents() throws {
    let url = try #require(Bundle.module.url(forResource: "events", withExtension: "txt", subdirectory: "Fixtures"))
    let lines = try String(contentsOf: url, encoding: .utf8).split(separator: "\n")
    let events = lines.compactMap { HostClient.parseEvent(Data($0.dropFirst(5).utf8)) }
    #expect(events.count == 6)
    guard case let .hello(hostId, rev, usage, _) = events[0] else { Issue.record("hello"); return }
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

@Test func parsesScreenEvents() throws {
    let e = HostClient.parseEvent(Data(#"{"type":"screen","screen":{"enabled":true,"connected":true,"capture":true,"displays":[{"id":1,"name":"Built-in","width":1512,"height":982,"main":true}],"agentBot":"b1"}}"#.utf8))
    guard case let .screen(s) = e else { Issue.record("screen"); return }
    #expect(s.available && s.agentBot == "b1" && s.mainDisplay?.width == 1512 && !s.userControl)
}

@Test func screenViewportMapsAndZooms() {
    // A 16:10 display in a wider landscape phone view: letterboxed left and right.
    var v = ScreenViewport(display: CGSize(width: 1600, height: 1000), view: CGSize(width: 800, height: 400))
    #expect(v.scale == 0.4)
    #expect(v.crop == nil)
    #expect(v.toDisplay(CGPoint(x: 400, y: 200)) == CGPoint(x: 800, y: 500))
    #expect(v.frame(of: v.displayBounds) == CGRect(x: 80, y: 0, width: 640, height: 400))

    // Pinch keeps the point under the fingers still.
    let anchor = CGPoint(x: 600, y: 100)
    let before = v.toDisplay(anchor)
    v.zoom(by: 2, at: anchor)
    let after = v.toDisplay(anchor)
    #expect(abs(before.x - after.x) < 0.001 && abs(before.y - after.y) < 0.001)
    let crop = v.crop!
    #expect(v.displayBounds.contains(crop))
    #expect(abs(crop.width / crop.height - 2) < 0.001)

    // Panning never leaves the display.
    v.pan(by: CGPoint(x: 10_000, y: 10_000))
    #expect(v.visible.minX == 0 && v.visible.minY == 0)

    // The cursor pulls the view along.
    v.follow(CGPoint(x: 1590, y: 990))
    #expect(v.visible.maxX == 1600 && v.visible.maxY == 1000)

    v.zoom(by: 1000, at: .zero)
    #expect(v.zoom == v.maxZoom && v.scale == 4)
}

@Test func tailscaleAddresses() {
    #expect(Tailscale.isAddress("http://100.101.2.3:19222"))
    #expect(Tailscale.isAddress("http://mac.tail1234.ts.net:19222"))
    #expect(!Tailscale.isAddress("http://100.200.2.3:19222"))
    #expect(!Tailscale.isAddress("http://192.168.1.5:19222"))
}

@Test func parsesClaudeResetText() throws {
    let taipei = try #require(TimeZone(identifier: "Asia/Taipei"))
    var cal = Calendar(identifier: .gregorian)
    cal.timeZone = taipei
    let now = try #require(cal.date(from: DateComponents(year: 2026, month: 9, day: 25, hour: 17, minute: 20)))
    let at = { (m: Int, d: Int, h: Int, min: Int, y: Int) in cal.date(from: DateComponents(year: y, month: m, day: d, hour: h, minute: min)) }
    #expect(UsageWindow.parseReset("Sep 25 at 7:30pm (Asia/Taipei)", now: now) == at(9, 25, 19, 30, 2026))
    #expect(UsageWindow.parseReset("Sep 26 at 12pm (Asia/Taipei)", now: now) == at(9, 26, 12, 0, 2026))
    #expect(UsageWindow.parseReset("Jan 2 at 12am (Asia/Taipei)", now: now) == at(1, 2, 0, 0, 2027))
    #expect(UsageWindow.parseReset("5am (Asia/Taipei)", now: now) == at(9, 26, 5, 0, 2026))
    #expect(UsageWindow.parseReset("soon") == nil)
}
