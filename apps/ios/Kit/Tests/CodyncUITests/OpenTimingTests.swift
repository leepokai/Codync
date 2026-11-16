import Foundation
import Testing
@testable import CodyncKit
@testable import CodyncUI

@MainActor @Test func oneOpenRecordsEachTimingOnceInOrder() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var clock = ContinuousClock.now
    var events: [PhonePerformance.OpenStep] = []
    let timing = OpenTiming(now: {
        clock += .milliseconds(1)
        return clock
    }, capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive()

    let computer = randomComputer("Mac")
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: computer, storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"catchUp":2}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    await remote.emit(#"{"type":"entry","entry":{"id":"e1","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"new"},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { events.count == 3 })
    _ = timing.record(.chatList, computer: computer.id)
    _ = timing.record(.openChat, computer: computer.id)
    _ = timing.record(.linkReady, computer: computer.id)
    _ = timing.record(.hello, computer: computer.id)
    _ = timing.record(.catchUp, computer: computer.id, chats: 1, messages: 1)
    _ = timing.record(.chatList, computer: computer.id)
    _ = timing.record(.openChat, computer: computer.id)

    #expect(events.count == 5)
    let phases = events.map(\.phase)
    #expect(phases == [.linkReady, .hello, .catchUp, .chatList, .openChat])
    let milliseconds = events.map(\.ms)
    #expect(milliseconds == [1, 2, 3, 4, 5])
    for event in events {
        #expect(event.open == .cold)
    }
    let catchUp = events[2]
    #expect(catchUp.chats == 1)
    #expect(catchUp.messages == 1)
}

@MainActor @Test func returningFromBackgroundStartsANewTimingOpen() {
    var clock = ContinuousClock.now
    var events: [PhonePerformance.OpenStep] = []
    let timing = OpenTiming(now: {
        clock += .milliseconds(1)
        return clock
    }, capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive()
    _ = timing.record(.linkReady, computer: "computer")
    timing.becameActive()
    #expect(events.count == 1)
    timing.enteredBackground()
    timing.becameActive()
    _ = timing.record(.linkReady, computer: "computer")

    guard events.count == 2 else {
        Issue.record("missing open timings")
        return
    }
    #expect(events[0].open == .cold)
    #expect(events[1].open == .foreground)
    #expect(events[0].ms == 1)
    #expect(events[1].ms == 1)
}

@MainActor @Test func foregroundReturnWithVoiceCallReportsARetainedLinkCaughtUpNothing() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [PhonePerformance.OpenStep] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive()

    let computer = randomComputer("Mac")
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: computer, storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    await remote.emit(botEvent("b2", name: "Other", rev: 2))
    #expect(await until { events.count == 3 && store.roster.count == 2 })

    store.beginVoiceCall(UUID(), hear: { _ in }, end: {})
    store.setActive(false)
    timing.enteredBackground()
    timing.becameActive()
    store.setActive(true)

    #expect(await until { events.count == 6 })
    let phases = events.map(\.phase)
    #expect(phases == [.linkReady, .hello, .catchUp, .linkReady, .hello, .catchUp])
    #expect(await remote.eventSubscriptions == 1)
    #expect(events[5].open == .foreground)
    #expect(events[5].chats == 0)
    #expect(events[5].messages == 0)
}

@MainActor @Test func foregroundReturnAfterARetainedLinkDroppedWaitsForReconnect() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [PhonePerformance.OpenStep] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive()

    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { events.count == 3 })

    store.beginVoiceCall(UUID(), hear: { _ in }, end: {})
    store.setActive(false)
    await remote.set(.connecting)
    #expect(await until { store.hostRoute == nil })
    timing.enteredBackground()
    timing.becameActive()
    store.setActive(true)

    #expect(events.count == 3)
}

@MainActor @Test func catchUpCountsAChatRepresentedOnlyByAnEntry() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [PhonePerformance.OpenStep] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive()
    let computer = randomComputer("Mac")
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: computer, storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(#"{"type":"entry","entry":{"id":"e1","seq":1,"botId":"entry-only","rev":1,"kind":"agent","turn":1,"data":{"text":"new"},"createdAt":1,"updatedAt":1}}"#)

    #expect(await until { events.count == 3 })
    #expect(events[2].chats == 1)
    #expect(events[2].messages == 1)
}

@MainActor @Test func foregroundReturnDuringCatchUpWaitsForTheCurrentSubscription() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [PhonePerformance.OpenStep] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive()
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: randomComputer("Mac"), storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    #expect(await until { events.count == 3 })

    store.beginVoiceCall(UUID(), hear: { _ in }, end: {})
    store.setActive(false)
    await remote.failEvents()
    #expect(await until { await remote.eventSubscriptions == 2 })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"catchUp":1}"#)
    timing.enteredBackground()
    timing.becameActive()
    store.setActive(true)
    #expect(events.count == 5)

    await remote.emit(botEvent("b1", name: "Bot", rev: 2))
    #expect(await until { events.count == 6 })
    #expect(events[5].phase == .catchUp)
    #expect(events[5].open == .foreground)
    #expect(events[5].chats == 1)
    #expect(events[5].messages == 0)
}

@MainActor @Test func catchUpCountsExcludeARetryAndIncludeChatsWithoutEntries() async throws {
    let (storage, suite) = context()
    defer { UserDefaults(suiteName: suite)?.removePersistentDomain(forName: suite) }
    var events: [PhonePerformance.OpenStep] = []
    let timing = OpenTiming(capture: { events.append($0) })
    timing.beginCold()
    timing.becameActive()

    let computer = randomComputer("Mac")
    let remote = FakeRemote(.ready(.direct))
    let store = BotStore(computer: computer, storage: storage) { _ in remote }
    store.openTiming = timing
    store.setActive(true)
    defer { store.retire() }
    #expect(await until { await remote.subscribed })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":2,"catchUp":3}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))
    await remote.emit(#"{"type":"entry","entry":{"id":"e1","seq":1,"botId":"b1","rev":2,"kind":"agent","turn":1,"data":{"text":"new"},"createdAt":1,"updatedAt":1}}"#)
    #expect(await until { !store.chat("b1").isEmpty })

    await remote.failEvents()
    #expect(await until { await remote.eventSubscriptions == 2 })
    await remote.emit(#"{"type":"hello","hostId":"h1","rev":1,"catchUp":1}"#)
    await remote.emit(botEvent("b1", name: "Bot", rev: 1))

    #expect(await until {
        events.contains { $0.phase == .catchUp }
    })
    let catchUp = try #require(events.first { $0.phase == .catchUp })
    #expect(catchUp.chats == 1)
    #expect(catchUp.messages == 0)
}
