import Testing
@testable import CodyncUI

@Test func computerFilterSupportsMultipleAndSingleSelections() {
    let all = ["a", "b", "c"]
    let initial = ComputerSelection(all: all, hidden: "")
    #expect(initial.shown == all)
    let withoutB = ComputerSelection(all: all, hidden: initial.toggling("b"))
    #expect(withoutB.shown == ["a", "c"])
    let onlyC = ComputerSelection(all: all, hidden: withoutB.only("c"))
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

@Test func startupCountsSSHBeforeAttachmentAndHandsOffWithoutDoubleCounting() {
    let signingIn = ComputerConnectionProgress(computerId: nil, name: "Remote", detail: "Signing in…")
    #expect(ConnectionSummary(computers: [:], progress: [signingIn]).progressText == "1 connecting…")

    let known = ComputerConnectionProgress(computerId: "remote", name: "Remote", detail: "Opening tunnel…")
    #expect(ConnectionSummary(computers: [:], progress: [known]).progressText == "1 connecting…")
    #expect(ConnectionSummary(computers: ["remote": .connecting], progress: [known]).progressText == "1 connecting…")
    #expect(ConnectionSummary(computers: ["remote": .online], progress: [known]).progressText == "1 connected")
    #expect(ConnectionSummary(computers: ["remote": .online], progress: []).progressText == "1 connected")
}

@Test func startupDistinguishesConnectingFromOfflineAndPreservesAnOnlineRoute() {
    let retrying = ComputerConnectionProgress(computerId: "remote", name: "Remote", detail: "Retrying…")
    #expect(ConnectionSummary(computers: ["remote": .offline("timeout")], progress: [retrying]).progressText == "1 connecting…")
    #expect(ConnectionSummary(computers: ["remote": .offline("timeout")], progress: []).progressText == "1 offline")
    #expect(ConnectionSummary(computers: ["remote": .online], progress: [retrying]).progressText == "1 connected")
    #expect(ConnectionSummary(computers: ["local": .online], progress: [retrying]).progressText == "1 connected · 1 connecting…")
    #expect(ConnectionSummary(computers: [:], progress: []).progressText == "Computers")
}
