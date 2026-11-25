import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@Test func burstyLiveChunkPacesMonotonicallyWithoutSnapping() {
    let payload = String(repeating: "Word ", count: 24)
    var state = StreamTextState(text: payload, live: true, now: 1.0)
    #expect(state.shownCount == 0)
    #expect(state.needsTick)

    var now = 1.0
    var steps = 0
    var previous = 0
    while state.needsTick && steps < 100 {
        now += 0.028
        _ = state.step(now: now)
        #expect(state.shownCount >= previous)
        #expect(state.shownCount <= payload.count)
        #expect(payload.hasPrefix(state.displayedText))
        previous = state.shownCount
        steps += 1
    }

    #expect(steps > 1)
    #expect(!state.needsTick)
    #expect(state.displayedText == payload)
}

@Test func arrivalRevealBoundsShortAndLongCompletions() {
    #expect(StreamTextState.arrivalRevealMin == 0.360)
    #expect(StreamTextState.arrivalRevealPerChar == 0.0032)
    #expect(StreamTextState.arrivalRevealMax == 5.200)

    let shortDuration = StreamTextState.arrivalRevealDuration(length: 12)
    let thousandCharDuration = StreamTextState.arrivalRevealDuration(length: 1_000)
    let longDuration = StreamTextState.arrivalRevealDuration(length: 5_000)
    #expect(shortDuration >= StreamTextState.arrivalRevealMin)
    #expect(shortDuration < 0.50)
    #expect(thousandCharDuration > 3.0 && thousandCharDuration < StreamTextState.arrivalRevealMax)
    #expect(longDuration == StreamTextState.arrivalRevealMax)

    var state = StreamTextState(text: "Short reply!", live: false, now: 0, arrivalReveal: true)
    #expect(state.needsTick)
    var now = 0.0
    while state.needsTick && now < 1.0 {
        now += 0.028
        _ = state.step(now: now)
    }
    #expect(!state.needsTick)
    #expect(state.displayedText == "Short reply!")
    #expect(now <= 0.50)
}

@Test func liveToFinalTransitionCatchesUpSmoothlyWithinBudget() {
    let text = String(repeating: "token ", count: 40)
    var state = StreamTextState(text: "tok", live: true, now: 0.0)
    _ = state.step(now: 0.028)
    state.sync(text: text, live: false, now: 0.100)
    #expect(state.finalAt == 0.100)
    #expect(state.needsTick)

    var now = 0.100
    while state.needsTick && now < 1.5 {
        now += 0.028
        _ = state.step(now: now)
    }
    #expect(!state.needsTick)
    #expect(state.displayedText == text)
    #expect(now - 0.100 <= StreamTextState.catchUpTarget + 0.06)
}

@Test func maxLagExpiresStaleCheckpoints() {
    let full = String(repeating: "x", count: 500)
    var state = StreamTextState(text: full, live: true, now: 0.0)
    #expect(state.checkpoints == [StreamTextState.LagCheckpoint(count: 500, at: 0.0)])

    let advanced = state.step(now: StreamTextState.maxLag + 0.05)
    #expect(advanced)
    #expect(state.shownCount == 500)
    #expect(!state.needsTick)
}

@Test func graphemeAndEmojiPrefixesStayValidAtEveryStep() {
    let text = "Hi 👨‍👩‍👧‍👦 中文测试 🚀 done"
    var state = StreamTextState(text: text, live: false, now: 0, arrivalReveal: true)
    var now = 0.0
    while state.needsTick {
        now += 0.028
        _ = state.step(now: now)
        #expect(text.hasPrefix(state.displayedText))
    }
    #expect(state.displayedText == text)
}

@Test func hiddenAndConvergeSnapImmediatelyWithoutTimers() {
    let text = "Immediate text under reduced motion"
    var hiddenState = StreamTextState(text: text, live: true, now: 0, hidden: true)
    #expect(hiddenState.displayedText == text)
    #expect(!hiddenState.needsTick)

    var visible = StreamTextState(text: text, live: false, now: 0, arrivalReveal: true)
    #expect(visible.needsTick)
    visible.converge()
    #expect(!visible.needsTick)
    #expect(visible.displayedText == text)
}

@MainActor @Test func botStoreMarksOnlyPostCatchUpLiveTextEntriesOnce() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }

    let computer = randomComputer("Mac")
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: computer, storage: storage) { _ in remote }
    defer { store.retire() }

    // Seed an entry loaded from cache before connection starts.
    let cachedEntry = Entry(
        id: "cached-1",
        seq: 1,
        botId: "b1",
        rev: 1,
        kind: "agent",
        turn: 1,
        data: EntryData(text: "From cache", status: nil, clientNonce: nil),
        createdAt: 1,
        updatedAt: 1
    )
    var cached = cachedEntry
    cached.data.final = true
    store.upsert(cached)
    #expect(!store.shouldRevealLiveText(for: cached))

    let readingToken = UUID()
    store.setReading(readingToken, botId: "b1", active: true)
    store.setActive(true)
    #expect(await until { await remote.subscribed })

    // Catch-up entries (catchUp: 2) must not trigger live reveal.
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":3,"usage":{"providers":[]},"catchUp":2}"#)
    await remote.emit(botEvent("b1", name: "Cody", rev: 1))
    await remote.emit(#"{"type":"entry","entry":{"id":"catchup-1","seq":2,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"Catch-up reply","final":true},"createdAt":2,"updatedAt":2}}"#)
    #expect(await until { store.streamCaughtUp })

    let catchUpEntry = try #require(store.chat("b1").first { $0.id == "catchup-1" })
    #expect(!store.shouldRevealLiveText(for: catchUpEntry))

    // Live SSE arrival after catch-up is marked once.
    await remote.emit(#"{"type":"entry","entry":{"id":"live-1","seq":3,"botId":"b1","rev":4,"kind":"agent","turn":2,"data":{"text":"Live final reply","final":true},"createdAt":3,"updatedAt":3}}"#)
    #expect(await until { store.chat("b1").contains { $0.id == "live-1" } })
    let liveEntry = try #require(store.chat("b1").first { $0.id == "live-1" })
    #expect(store.shouldRevealLiveText(for: liveEntry))

    // Once reveal finishes, reaction-only updates do not replay it.
    store.completeLiveReveal(id: "live-1")
    #expect(!store.shouldRevealLiveText(for: liveEntry))
    await remote.emit(#"{"type":"entry","entry":{"id":"live-1","seq":3,"botId":"b1","rev":5,"kind":"agent","turn":2,"data":{"text":"Live final reply","final":true,"reactions":["👍"]},"createdAt":3,"updatedAt":4}}"#)
    #expect(await until { store.chat("b1").first { $0.id == "live-1" }?.rev == 5 })
    let reactedEntry = try #require(store.chat("b1").first { $0.id == "live-1" })
    #expect(!store.shouldRevealLiveText(for: reactedEntry))

    // Live thought arrival is eligible; artifact/voice/file card arrivals are not.
    await remote.emit(#"{"type":"entry","entry":{"id":"thought-1","seq":4,"botId":"b1","rev":6,"kind":"thought","turn":3,"data":{"text":"Thinking step"},"createdAt":4,"updatedAt":4}}"#)
    await remote.emit(#"{"type":"entry","entry":{"id":"file-1","seq":5,"botId":"b1","rev":7,"kind":"agent","turn":3,"data":{"text":"","final":true,"files":[{"id":"f1","name":"out.txt","size":12,"sha256":"abc"}]},"createdAt":5,"updatedAt":5}}"#)
    #expect(await until { store.rev >= 7 })

    let thoughtEntry = try #require(store.entries["b1"]?.first { $0.id == "thought-1" })
    let fileEntry = try #require(store.entries["b1"]?.first { $0.id == "file-1" })
    #expect(store.shouldRevealLiveText(for: thoughtEntry))
    #expect(!store.shouldRevealLiveText(for: fileEntry))
}

@MainActor @Test func liveProgressResumesWithoutReplayingRecycledCell() {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }

    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in FakeRemote(.ready(.direct)) }
    defer { store.retire() }

    store.liveArrivalIDs.insert("msg-1")
    var entry = Entry(id: "msg-1", seq: 1, botId: "b1", rev: 1, kind: "agent", turn: 1, data: EntryData(text: "Hello streaming world"), createdAt: 1, updatedAt: 1)
    entry.data.final = true

    #expect(store.shouldRevealLiveText(for: entry))
    store.noteLiveRevealProgress(id: "msg-1", count: 6)
    #expect(store.initialRevealedCount(for: entry) == 6)

    let resumed = StreamTextState(
        text: entry.data.text ?? "",
        live: false,
        now: 1.0,
        arrivalReveal: true,
        initialShownCount: store.initialRevealedCount(for: entry)
    )
    #expect(resumed.shownCount == 6)
    #expect(resumed.displayedText == "Hello ")

    store.completeLiveReveal(id: "msg-1")
    #expect(!store.shouldRevealLiveText(for: entry))
    #expect(store.initialRevealedCount(for: entry) == nil)
}

@MainActor @Test func closedChatAndExpiredLiveArrivalsDoNotReplayLater() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }

    var now: TimeInterval = 100.0
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
    defer { store.retire() }
    store.revealClock = { now }

    let openToken = UUID()
    store.setReading(openToken, botId: "b1", active: true)
    store.setActive(true)
    #expect(await until { await remote.subscribed })

    await remote.emit(#"{"type":"hello","hostId":"h1","rev":0,"usage":{"providers":[]},"catchUp":0}"#)
    #expect(await until { store.streamCaughtUp })

    // Closed chat b2 receives a finished reply: must not be marked for reveal when opened later.
    await remote.emit(#"{"type":"entry","entry":{"id":"closed-1","seq":1,"botId":"b2","rev":1,"kind":"agent","turn":1,"data":{"text":"Finished in background chat","final":true},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.chat("b2").contains { $0.id == "closed-1" } })
    let closedEntry = try #require(store.chat("b2").first { $0.id == "closed-1" })
    #expect(!store.shouldRevealLiveText(for: closedEntry))

    // Open chat b1 receives a live reply, but if unmounted past TTL it expires cleanly.
    await remote.emit(#"{"type":"entry","entry":{"id":"ttl-1","seq":2,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"Visible live reply","final":true},"createdAt":2,"updatedAt":2}}"#)
    #expect(await until { store.chat("b1").contains { $0.id == "ttl-1" } })
    let ttlEntry = try #require(store.chat("b1").first { $0.id == "ttl-1" })
    #expect(store.shouldRevealLiveText(for: ttlEntry))

    now += BotStore.liveArrivalTTL + 1.0
    #expect(!store.shouldRevealLiveText(for: ttlEntry))
    #expect(!store.liveArrivalIDs.contains("ttl-1"))

    // Leaving an open chat clears its pending live reveals immediately.
    await remote.emit(#"{"type":"entry","entry":{"id":"leave-1","seq":3,"botId":"b1","rev":3,"kind":"agent","turn":2,"data":{"text":"Leaving mid-stream"},"createdAt":3,"updatedAt":3}}"#)
    #expect(await until { store.chat("b1").contains { $0.id == "leave-1" } })
    let leaveEntry = try #require(store.chat("b1").first { $0.id == "leave-1" })
    #expect(store.shouldRevealLiveText(for: leaveEntry))
    store.setReading(openToken, botId: "b1", active: false)
    #expect(!store.shouldRevealLiveText(for: leaveEntry))
}

@MainActor @Test func deletedEntriesAndDisconnectClearRevealProgress() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }

    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
    defer { store.retire() }

    let token = UUID()
    store.setReading(token, botId: "b1", active: true)
    store.setActive(true)
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":0,"usage":{"providers":[]},"catchUp":0}"#)
    #expect(await until { store.streamCaughtUp })

    await remote.emit(botEvent("b1", name: "Cody", rev: 1))
    await remote.emit(#"{"type":"entry","entry":{"id":"del-1","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"About to be deleted"},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.chat("b1").contains { $0.id == "del-1" } })
    store.noteLiveRevealProgress(id: "del-1", count: 5)
    #expect(store.liveArrivalIDs.contains("del-1"))
    #expect(store.revealedTextByID["del-1"] == 5)

    await remote.emit(#"{"type":"bot","rev":3,"bot":{"id":"b1","deleted":true,"rev":3}}"#)
    #expect(await until { store.bots["b1"] == nil })
    #expect(!store.liveArrivalIDs.contains("del-1"))
    #expect(store.revealedTextByID["del-1"] == nil)

    // Going inactive clears any remaining live reveal state.
    store.liveArrivalIDs.insert("dangling-1")
    store.revealedTextByID["dangling-1"] = 4
    store.setActive(false)
    #expect(store.liveArrivalIDs.isEmpty)
    #expect(store.revealedTextByID.isEmpty)
}

@MainActor @Test func legacyHostWithoutCatchUpCountSyncsWithoutReplayingHistory() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }

    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
    defer { store.retire() }

    let token = UUID()
    store.setReading(token, botId: "b1", active: true)
    store.setActive(true)
    #expect(await until { await remote.subscribed })

    // Pre-2.32.0 host omits `catchUp`: sync finishes normally (`streamCaughtUp == true`),
    // but historical entries sent after hello must not replay.
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"usage":{"providers":[]}}"#)
    await remote.emit(#"{"type":"entry","entry":{"id":"old-1","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"Historical reply","final":true},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { store.chat("b1").contains { $0.id == "old-1" } })
    #expect(store.streamCaughtUp)
    let oldEntry = try #require(store.chat("b1").first { $0.id == "old-1" })
    #expect(!store.shouldRevealLiveText(for: oldEntry))

    // Upgrading/reconnecting with an explicit `catchUp` boundary enables live reveal for new arrivals.
    await remote.endEvents()
    #expect(await until { !store.streamCaughtUp })
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"usage":{"providers":[]},"catchUp":0}"#)
    #expect(await until { store.streamCaughtUp })
    await remote.emit(#"{"type":"entry","entry":{"id":"new-1","seq":2,"botId":"b1","rev":3,"kind":"agent","turn":2,"data":{"text":"Live after upgrade","final":true},"createdAt":2,"updatedAt":2}}"#)
    #expect(await until { store.chat("b1").contains { $0.id == "new-1" } })
    let newEntry = try #require(store.chat("b1").first { $0.id == "new-1" })
    #expect(store.shouldRevealLiveText(for: newEntry))
}
