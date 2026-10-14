import Foundation
import Testing
@testable import CodyncKit

@MainActor
@Test func analyticsIsOnByDefaultAndIdentifiesAccountsOnce() throws {
    let defaults = try #require(UserDefaults(suiteName: "AnalyticsTests-\(UUID())"))
    let analytics = Analytics(defaults: defaults) { _ in false }

    // On until turned off; off drops what's queued and records nothing more.
    #expect(analytics.isEnabled)
    analytics.capture(.appOpened)
    #expect(analytics.pending.map(\.event) == ["app_opened"])
    analytics.setEnabled(false)
    analytics.capture(.appOpened)
    #expect(analytics.pending.isEmpty)

    // Signed out: the install ID with its own person profile, the common properties.
    analytics.setEnabled(true)
    analytics.capture(.computerPaired, ["via": "qr"])
    let anon = try #require(analytics.pending.first)
    #expect(anon.event == "computer_paired")
    #expect(anon.properties["via"] == .string("qr"))
    #expect(anon.properties["$process_person_profile"] == .bool(true))
    #expect(anon.properties["$lib"] == .string("codync-ios"))
    #expect(anon.properties["$os"] == .string("iOS"))
    #expect(anon.properties["$geoip_disable"] == nil)
    #expect(anon.properties["env"] != nil && anon.properties["app_version"] != nil)

    // Signed in: $identify links the install ID once, then events carry the user ID.
    analytics.userID = { "user_1" }
    analytics.capture(.signedIn)
    analytics.capture(.voiceCallStarted)
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

@MainActor
@Test func crashReportsSendNamesAndFramesButNoMessage() throws {
    // MetricKit's call-stack JSON: the root frame crashed, each first sub-frame is its caller.
    func frame(_ binary: String, _ offset: Int, _ caller: String = "") -> String {
        #"{"binaryName":"\#(binary)","offsetIntoBinaryTextSegment":\#(offset),"binaryUUID":"U","address":1,"sampleCount":1\#(caller.isEmpty ? "" : #","subFrames":[\#(caller)]"#)}"#
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

    // Unknown numbers fall back to generic names; nothing but names and offsets is sent.
    let unknown = CrashReports.properties(callStack: Data("{}".utf8), exceptionType: 99, signal: nil, appVersion: "1")
    let encoded = String(decoding: try JSONEncoder().encode(unknown), as: UTF8.self)
    #expect(encoded.contains(#""type":"Crash""#) && encoded.contains(#""frames":[]"#))

    // A runaway stack keeps the 50 frames nearest the crash.
    var deep = frame("Codync", 0)
    for i in 1...80 { deep = frame("Codync", i, deep) }
    let capped = CrashReports.frames(Data(#"{"callStacks":[{"callStackRootFrames":[\#(deep)]}]}"#.utf8))
    #expect(capped.count == 50)
    #expect(capped.last == .object(["platform": .string("custom"), "lang": .string("swift"), "function": .string("Codync + 80"),
                                    "module": .string("Codync"), "resolved": .bool(false), "in_app": .bool(true)]))

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
