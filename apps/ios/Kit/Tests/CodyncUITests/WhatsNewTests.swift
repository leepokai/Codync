@testable import CodyncKit
import Foundation
import Testing
@testable import CodyncUI

private let announcements = [
    Announcement(version: "2.46.0", notes: ["C"]),
    Announcement(version: "2.45.0", notes: ["B"]),
    Announcement(version: "2.44.0", notes: ["A"]),
]

private func due(seen: String?, current: String, usedBefore: Bool = true) -> [String] {
    Announcement.due(announcements, seen: seen, current: current, usedBefore: usedBefore).map(\.version)
}

@Test func whatsNewShowsReleasesSinceTheLastOneSeenNewestFirst() {
    #expect(due(seen: "2.43.2", current: "2.45.1") == ["2.45.0", "2.44.0"])
    #expect(due(seen: "2.44.0", current: "2.46.0") == ["2.46.0", "2.45.0"])
    #expect(due(seen: "2.46.0", current: "2.46.0") == [])
    #expect(due(seen: "2.47.0", current: "2.46.0") == [], "a downgrade shows nothing")
}

@Test func whatsNewWithoutARecordDependsOnWhetherTheDeviceWasSetUp() {
    #expect(due(seen: nil, current: "2.44.0", usedBefore: false) == [], "a new install")
    #expect(due(seen: nil, current: "2.44.0", usedBefore: true) == ["2.44.0"], "an update from before the record")
    #expect(due(seen: "2.43.0", current: "", usedBefore: true) == [], "a bundle without a version")
}

@Test func anAnnouncementReadsItsFeaturesAndNotes() throws {
    let text = #"[{"version": "2.44.0", "features": [{"title": "T", "body": "B", "beta": true, "media": "m", "film": "https://example.com/f"}, {"title": "U", "body": "C"}]}, {"version": "2.43.0", "notes": ["N"]}]"#
    let film = try #require(URL(string: "https://example.com/f"))
    #expect(try JSONDecoder().decode([Announcement].self, from: Data(text.utf8)) == [
        Announcement(version: "2.44.0", features: [.init(title: "T", body: "B", beta: true, media: "m", film: film), .init(title: "U", body: "C")]),
        Announcement(version: "2.43.0", notes: ["N"]),
    ])
}

@Test func theBundledAnnouncementsLoadWithTheirClipsAndEveryPhraseIsInEveryCatalog() throws {
    let all = try Announcement.load()
    for feature in all.flatMap(\.features) where feature.media != nil { #expect(feature.clip != nil, "\(feature.title)") }
    let notes = all.flatMap(\.phrases)
    let languages = URL(filePath: #filePath).deletingLastPathComponent()
        .appending(path: "../../Sources/CodyncKit/Resources/Languages").standardized
    let catalogs = try FileManager.default.contentsOfDirectory(at: languages, includingPropertiesForKeys: nil)
        .filter { $0.pathExtension == "json" }
    #expect(!catalogs.isEmpty)
    for url in catalogs {
        let catalog = try JSONDecoder().decode([String: String].self, from: Data(contentsOf: url))
        for note in notes { #expect(catalog[note] != nil, "\(url.lastPathComponent): \(note)") }
    }
}

@MainActor
private final class Events {
    var names: [String] = []
}

@MainActor
@Test func whatsNewRecordsAtOnceWhenNothingIsDueElseOnceShown() throws {
    let suite = UUID().uuidString
    let defaults = try #require(UserDefaults(suiteName: suite))
    defer { defaults.removePersistentDomain(forName: suite) }
    let events = Events()
    let capture: @MainActor (AnalyticsEvent) -> Void = { events.names.append($0.name) }

    let quiet = WhatsNew(usedBefore: false, defaults: defaults, current: "2.44.0", all: announcements, capture: capture)
    #expect(!quiet.isPresented)
    #expect(defaults.string(forKey: WhatsNew.seenKey) == "2.44.0")

    let update = WhatsNew(usedBefore: false, defaults: defaults, current: "2.46.0", all: announcements, capture: capture)
    #expect(update.isPresented)
    #expect(update.due.map(\.version) == ["2.46.0", "2.45.0"])
    #expect(defaults.string(forKey: WhatsNew.seenKey) == "2.44.0", "not before it's on screen")
    update.shown()
    update.shown()
    #expect(defaults.string(forKey: WhatsNew.seenKey) == "2.46.0")
    #expect(events.names == ["whats_new_shown"])

    #expect(!WhatsNew(usedBefore: true, defaults: defaults, current: "2.46.0", all: announcements, capture: capture).isPresented)
}
