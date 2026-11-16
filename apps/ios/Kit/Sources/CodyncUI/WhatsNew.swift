import CodyncKit
import Foundation
import Observation
import os

/// A release's highlights, shown once after updating to it (docs/features/whats-new.md).
public struct Announcement: Decodable, Equatable, Identifiable, Sendable {
    /// One highlight: a short preview clip over its name and a sentence about it.
    public struct Feature: Decodable, Equatable, Identifiable, Sendable {
        /// English interface phrases, translated in every catalog.
        public let title: String
        public let body: String
        /// Still in beta: a tag beside the title.
        public let beta: Bool
        /// The name of its muted, looping clip (`Resources/Announcements/<media>.mp4`).
        public let media: String?
        /// Where the full video plays.
        public let film: URL?
        public var id: String { title }

        public init(title: String, body: String, beta: Bool = false, media: String? = nil, film: URL? = nil) {
            self.title = title
            self.body = body
            self.beta = beta
            self.media = media
            self.film = film
        }

        public init(from decoder: any Decoder) throws {
            let values = try decoder.container(keyedBy: CodingKeys.self)
            try self.init(title: values.decode(String.self, forKey: .title), body: values.decode(String.self, forKey: .body),
                          beta: values.decodeIfPresent(Bool.self, forKey: .beta) ?? false,
                          media: values.decodeIfPresent(String.self, forKey: .media),
                          film: values.decodeIfPresent(URL.self, forKey: .film))
        }

        private enum CodingKeys: String, CodingKey { case title, body, beta, media, film }

        /// The bundled clip, when the file is there.
        public var clip: URL? {
            media.flatMap { Bundle.module.url(forResource: $0, withExtension: "mp4", subdirectory: Announcement.folder) }
        }
    }

    public let version: String
    public let features: [Feature]
    /// Smaller changes, text only: English interface phrases, translated in every catalog.
    public let notes: [String]
    public var id: String { version }

    public init(version: String, features: [Feature] = [], notes: [String] = []) {
        self.version = version
        self.features = features
        self.notes = notes
    }

    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        try self.init(version: values.decode(String.self, forKey: .version),
                      features: values.decodeIfPresent([Feature].self, forKey: .features) ?? [],
                      notes: values.decodeIfPresent([String].self, forKey: .notes) ?? [])
    }

    private enum CodingKeys: String, CodingKey { case version, features, notes }

    /// Every phrase the window shows, for the catalogs.
    var phrases: [String] { features.flatMap { [$0.title, $0.body] } + notes }

    static let folder = "Announcements"
    private static let log = Logger(subsystem: "com.pokai.Codync", category: "WhatsNew")

    /// The bundled list (`Resources/Announcements/announcements.json`, shared with every client, beside its clips).
    /// A missing or malformed file is logged and shows nothing.
    public static let bundled: [Announcement] = {
        do { return try load() }
        catch {
            log.error("announcements.json failed to load: \(error, privacy: .public)")
            return []
        }
    }()

    static func load() throws -> [Announcement] {
        guard let url = Bundle.module.url(forResource: "announcements", withExtension: "json", subdirectory: folder) else {
            throw CocoaError(.fileNoSuchFile)
        }
        return try JSONDecoder().decode([Announcement].self, from: Data(contentsOf: url))
    }

    /// The ones to show on `current`, in the file's order (newest first): newer than `seen` and not newer than
    /// `current`. Without `seen`, a device set up before (`usedBefore`) counts from 0.0.0, a new install from `current`.
    public static func due(_ all: [Announcement] = bundled, seen: String?, current: String, usedBefore: Bool) -> [Announcement] {
        guard AppVersion.parse(current) != nil else { return [] }
        let since = seen ?? (usedBefore ? "0.0.0" : current)
        return all.filter { AppVersion.isBelow(since, $0.version) && !AppVersion.isBelow(current, $0.version) }
    }
}

/// This run's What's new window, decided once at launch: before onboarding can finish, so a new install never
/// counts as an update.
@MainActor @Observable
public final class WhatsNew {
    static let seenKey = "whatsNewSeen"
    /// Due on this run, newest first.
    public let due: [Announcement]
    /// Until it's closed.
    public var isPresented: Bool
    @ObservationIgnored private let defaults: UserDefaults
    @ObservationIgnored private let current: String
    @ObservationIgnored private let capture: @MainActor (AnalyticsEvent) -> Void
    @ObservationIgnored private var recorded = false

    /// `usedBefore`: this device finished onboarding before, so no record means an update from a version without it.
    public init(usedBefore: Bool, defaults: UserDefaults = .standard, current: String = AppVersion.current,
                all: [Announcement] = Announcement.bundled,
                capture: @escaping @MainActor (AnalyticsEvent) -> Void = { Analytics.shared.capture($0) }) {
        self.defaults = defaults
        self.current = current
        self.capture = capture
        due = Announcement.due(all, seen: defaults.string(forKey: Self.seenKey), current: current, usedBefore: usedBefore)
        isPresented = !due.isEmpty
        if due.isEmpty { record() }
    }

    /// On screen: not again on this version.
    public func shown() {
        guard !recorded else { return }
        record()
        capture(.whatsNewShown)
    }

    private func record() {
        recorded = true
        defaults.set(current, forKey: Self.seenKey)
    }
}
