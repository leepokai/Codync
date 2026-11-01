import Testing
@testable import CodyncUI

@Test func computerFilterShowsAndHidesComputersOneAtATime() {
    let all = ["a", "b", "c"]
    let initial = ComputerSelection(all: all, hidden: "")
    #expect(initial.shown == all)
    let withoutB = ComputerSelection(all: all, hidden: initial.toggling("b"))
    #expect(withoutB.shown == ["a", "c"])
    let onlyC = ComputerSelection(all: all, hidden: withoutB.toggling("a"))
    #expect(onlyC.shown == ["c"])
    #expect(ComputerSelection(all: all, hidden: onlyC.toggling("a")).shown == ["a", "c"])
}

@Test func computerFilterKeepsOneComputerAndRecoversFromRemovedComputers() {
    let single = ComputerSelection(all: ["a", "b"], hidden: "b")
    #expect(ComputerSelection(all: single.all, hidden: single.toggling("a")).shown == ["a"])
    let removed = ComputerSelection(all: ["b", "c"], hidden: "b,c")
    #expect(removed.shown == ["b", "c"])
    #expect(ComputerSelection(all: removed.all, hidden: removed.toggling("b")).shown == ["c"])
    #expect(ComputerSelection(all: ["a", "b", "new"], hidden: "b").shown == ["a", "new"])
    #expect(ComputerSelection(all: [], hidden: "a").shown.isEmpty)
}

@Test func headerSummarizesOnlyTheSelectedConnections() {
    #expect(ConnectionSummary(connections: []).text == "Computers")
    #expect(ConnectionSummary(connections: [.online]).text == "1 connected")
    #expect(ConnectionSummary(connections: [.online, .online]).text == "2 connected")
    #expect(ConnectionSummary(connections: [.online, .computerOffline(lastSeen: nil)]).text == "1/2 connected")
    #expect(ConnectionSummary(connections: [.offline("timeout"), .computerOffline(lastSeen: nil)]).text == "2 offline")
    #expect(ConnectionSummary(connections: [.offline("timeout"), .connecting]).text == "Connecting…")
    #expect(ConnectionSummary(connections: [.unauthorized("revoked")]).text == "No access")
}
