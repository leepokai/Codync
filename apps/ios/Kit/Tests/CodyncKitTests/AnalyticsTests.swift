import Foundation
import Testing
@testable import CodyncKit

@MainActor
@Test func analyticsIsOnByDefaultAndIdentifiesAccountsOnce() throws {
    let defaults = try #require(UserDefaults(suiteName: "AnalyticsTests-\(UUID())"))
    let analytics = Analytics(defaults: defaults) { _ in false }

    // On until turned off; off drops what's queued and records nothing more.
    #expect(analytics.isEnabled)
    analytics.capture(.appOpened())
    #expect(analytics.pending.map(\.event) == ["app_opened"])
    analytics.setEnabled(false)
    analytics.capture(.appOpened())
    #expect(analytics.pending.isEmpty)

    // Signed out: the install ID with its own person profile, the common properties.
    analytics.setEnabled(true)
    analytics.capture(.computerPaired(via: .qr, computer: "AAECAwQFBgcICQoLDA0ODw"))
    let anon = try #require(analytics.pending.first)
    #expect(anon.event == "computer_paired")
    #expect(anon.properties["via"] == .string("qr"))
    #expect(anon.properties["computer"] == .string("2d6878bee20aba04"))
    #expect(anon.properties["$process_person_profile"] == .bool(true))
    #expect(anon.properties["$lib"] == .string("codync-ios"))
    #expect(anon.properties["$os"] == .string("iOS"))
    #expect(anon.properties["$geoip_disable"] == nil)
    #expect(anon.properties["env"] != nil && anon.properties["app_version"] != nil)

    // Signed in: $identify links the install ID once, then events carry the user ID.
    analytics.userID = { "user_1" }
    analytics.capture(.signedIn(.apple))
    analytics.capture(.voiceCallStarted(.onDevice))
    let names = analytics.pending.map(\.event)
    #expect(names == ["computer_paired", "$identify", "signed_in", "voice_call_started"])
    let identify = analytics.pending[1]
    #expect(identify.distinct_id == "user_1")
    #expect(identify.properties["$anon_distinct_id"] == .string(anon.distinct_id))
    #expect(analytics.pending[2].properties["$process_person_profile"] == .bool(true))

    // The computer key matches the host's (host/src/analytics.rs `computer_tag_matches_the_iphone`).
    #expect(Analytics.computerTag("AAECAwQFBgcICQoLDA0ODw") == "2d6878bee20aba04")

    // Turning it off drops what wasn't sent.
    analytics.setEnabled(false)
    #expect(analytics.pending.isEmpty)
}

@Test func eventPropertiesAreTypedAndUseTheCatalogNames() throws {
    func json(_ event: AnalyticsEvent) throws -> String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = .sortedKeys
        return String(decoding: try encoder.encode(event.properties), as: UTF8.self)
    }
    // Booleans and counts go out as JSON booleans and numbers, not strings.
    #expect(try json(.appOpened(notifications: .notAsked, widgets: 2)) == #"{"notifications":"not_asked","widgets":2}"#)
    #expect(try json(.appOpened()) == "{}")
    #expect(try json(.notificationsAnswered(granted: false, source: .widgets)) == #"{"granted":false,"source":"widgets"}"#)
    #expect(try json(.settingChanged(.liveActivities, on: true)) == #"{"on":true,"setting":"live_activities"}"#)
    // Multi-word values are snake_case, as in the catalog.
    #expect(try json(.signInFailed(.google, reason: .cancelled)) == #"{"provider":"google","reason":"cancelled"}"#)
    #expect(try json(.screenViewed(.botDetails)) == #"{"screen":"bot_details"}"#)
    #expect(try json(.voiceCallStarted(.onDevice)) == #"{"engine":"on_device"}"#)
    #expect(try json(.voiceCallQuality(.gemini, readyMs: 1200, reconnects: 1, stuck: .none, failure: .connection))
        == #"{"engine":"gemini","failure":"connection","ready_ms":1200,"reconnects":1,"stuck":"none"}"#)
    // An engine that was never ready leaves `ready_ms` out.
    #expect(try json(.voiceCallQuality(.onDevice, readyMs: nil, reconnects: 0, stuck: .connecting, failure: .none))
        == #"{"engine":"on_device","failure":"none","reconnects":0,"stuck":"connecting"}"#)
    #expect(try json(.linkOpened(.installCommand)) == #"{"link":"install_command"}"#)
    #expect(AnalyticsEvent.notificationOpened(.needsInput).name == "notification_opened")
    #expect(AnalyticsEvent.NotificationKind(category: "needsInput") == .needsInput)
    #expect(AnalyticsEvent.NotificationKind(category: "other") == nil)

    // Why pairing failed: a category, never the error's text; called off isn't a failure.
    #expect(AnalyticsEvent.PairingFailure(Pairing.Problem.outdatedHost) == .outdatedHost)
    #expect(AnalyticsEvent.PairingFailure(HostError.http(401, "secret")) == .unauthorized)
    #expect(AnalyticsEvent.PairingFailure(HostError.http(403, "This pairing code expired.")) == .expired)
    #expect(AnalyticsEvent.PairingFailure(HostError.computerOffline(lastSeen: nil)) == .unreachable)
    #expect(AnalyticsEvent.PairingFailure(URLError(.timedOut)) == .timeout)
    #expect(AnalyticsEvent.PairingFailure(CocoaError(.fileNoSuchFile)) == .other)
    #expect(AnalyticsEvent.PairingFailure(CancellationError()) == nil)
}

@MainActor
@Test func crashReportsSendNamesAndFramesButNoMessage() throws {
    // MetricKit's call-stack JSON: the root frame crashed, each first sub-frame is its caller.
    let loadedAt = ["Codync": 0x1_0480_0000, "SwiftUI": 0x1_8a00_0000, "CodyncKit": 0x1_0600_0000]
    let uuids = ["Codync": "d97c7f78-3fcc-389d-9047-deafc1d7bdc3", "SwiftUI": "11111111-2222-3333-4444-555555555555",
                 "CodyncKit": "AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE"]
    func frame(_ binary: String, _ offset: Int, _ caller: String = "") -> String {
        let address = (loadedAt[binary] ?? 0) + offset
        return #"{"binaryName":"\#(binary)","offsetIntoBinaryTextSegment":\#(offset),"binaryUUID":"\#(uuids[binary] ?? "")","address":\#(address),"sampleCount":1\#(caller.isEmpty ? "" : #","subFrames":[\#(caller)]"#)}"#
    }
    let crashed = frame("CodyncKit", 3, frame("SwiftUI", 2, frame("Codync", 1)))
    let json = #"{"callStackPerThread":true,"callStacks":[{"threadAttributed":false,"callStackRootFrames":[\#(frame("libsystem_kernel.dylib", 9))]},{"threadAttributed":true,"callStackRootFrames":[\#(crashed)]}]}"#
    let p = CrashReports.properties(callStack: Data(json.utf8), exceptionType: 1, signal: 11, appVersion: "2.14.1")

    #expect(p["$exception_level"] == .string("fatal"))
    #expect(p["process"] == .string("ios"))
    #expect(p["app_version"] == .string("2.14.1"))
    guard case .array(let list) = p["$exception_list"], case .object(let exception) = list.first,
          case .object(let stack) = exception["stacktrace"], case .array(let frames) = stack["frames"] else {
        Issue.record("no exception")
        return
    }
    #expect(exception["type"] == .string("EXC_BAD_ACCESS"))
    #expect(exception["value"] == .string("SIGSEGV"))
    // The attributed thread, oldest first: the crashing frame is last.
    let functions = frames.compactMap { if case .object(let f) = $0 { f["function"] } else { nil } }
    #expect(functions == [.string("Codync + 1"), .string("SwiftUI + 2"), .string("CodyncKit + 3")])
    let inApp = frames.compactMap { if case .object(let f) = $0 { f["in_app"] } else { nil } }
    #expect(inApp == [.bool(true), .bool(false), .bool(true)])
    // PostHog symbolicates an address with the dSYM of the binary loaded at `image_addr`.
    #expect(frames.last == .object(["platform": .string("apple"), "function": .string("CodyncKit + 3"), "module": .string("CodyncKit"),
                                    "in_app": .bool(true), "instruction_addr": .string("0x106000003"), "image_addr": .string("0x106000000")]))
    // The binary's UUID as posthog-cli uploads it: uppercase.
    let image = { (name: String, at: String) in
        Analytics.Value.object(["type": .string("macho"), "debug_id": .string(uuids[name, default: ""].uppercased()), "image_addr": .string(at)])
    }
    #expect(p["$debug_images"] == .array([image("Codync", "0x104800000"), image("SwiftUI", "0x18a000000"), image("CodyncKit", "0x106000000")]))

    // Unknown numbers fall back to generic names; nothing but names and addresses is sent.
    let unknown = CrashReports.properties(callStack: Data("{}".utf8), exceptionType: 99, signal: nil, appVersion: "1")
    let encoded = String(decoding: try JSONEncoder().encode(unknown), as: UTF8.self)
    #expect(encoded.contains(#""type":"Crash""#) && encoded.contains(#""frames":[]"#) && unknown["$debug_images"] == nil)

    // A runaway stack keeps the 200 frames nearest the crash, all in one binary.
    var deep = frame("Codync", 0)
    for i in 1...250 { deep = frame("Codync", i, deep) }
    let capped = CrashReports.stack(Data(#"{"callStacks":[{"callStackRootFrames":[\#(deep)]}]}"#.utf8))
    #expect(capped.frames.count == 200 && capped.images.count == 1)
    #expect(capped.frames.last == .object(["platform": .string("apple"), "function": .string("Codync + 250"), "module": .string("Codync"),
                                           "in_app": .bool(true), "instruction_addr": .string("0x1048000fa"), "image_addr": .string("0x104800000")]))

    // Recorded like any other event: off records nothing.
    let defaults = try #require(UserDefaults(suiteName: "CrashReportsTests-\(UUID())"))
    let analytics = Analytics(defaults: defaults) { _ in false }
    analytics.record("$exception", p)
    #expect(analytics.pending.first?.properties["process"] == .string("ios"))
    #expect(analytics.pending.first?.properties["$lib"] == .string("codync-ios"))
    analytics.setEnabled(false)
    analytics.record("$exception", p)
    #expect(analytics.pending.isEmpty)
}

@MainActor
@Test func eventsShareASessionAndACrashGoesToTheLaunchItEnded() throws {
    let defaults = try #require(UserDefaults(suiteName: "AnalyticsSessionTests-\(UUID())"))
    // The first launch ever has no session to put a crash in.
    let first = Analytics(defaults: defaults) { _ in false }
    first.recordCrash([:])
    first.capture(.appOpened())
    first.capture(.voiceCallStarted(.cloud))
    let sessions = first.pending.map { $0.properties["$session_id"] }
    #expect(sessions[0] == nil && sessions[1] != nil && sessions[1] == sessions[2])

    // The next launch starts its own session; a crash MetricKit reports then belongs to the last one.
    let next = Analytics(defaults: defaults) { _ in false }
    next.recordCrash([:])
    next.capture(.appOpened())
    #expect(next.pending[0].properties["$session_id"] == sessions[1])
    #expect(next.pending[1].properties["$session_id"] != sessions[1])
}

@Test func aSessionEndsAfterHalfAnHourIdleOrADayInAll() {
    var session = AnalyticsSession()
    let t0 = Date(timeIntervalSince1970: 1_800_000_000)
    let first = session.id(at: t0)
    // A UUIDv7 whose first 48 bits are its start in milliseconds.
    let hex = first.replacingOccurrences(of: "-", with: "")
    #expect(hex.prefix(12) == String(format: "%012llx", UInt64(1_800_000_000_000)) && Array(hex)[12] == "7")
    #expect(session.id(at: t0 + 29 * 60) == first)
    let start = t0 + 60 * 60
    let second = session.id(at: start)
    #expect(second != first, "idle for 31 minutes")

    // Busy without a break: a new one once it has run for a day.
    var t = start
    while t <= start + 24 * 3600 {
        #expect(session.id(at: t) == second)
        t += 20 * 60
    }
    #expect(session.id(at: t) != second)
}
