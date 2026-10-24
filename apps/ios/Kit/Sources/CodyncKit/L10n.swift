import Foundation
import os

/// Which Chinese writing the interface uses. Traditional is its own table, not a conversion of Simplified.
public enum ChineseScript: Sendable {
    case hans
    case hant
}

/// The language of this app's own interface. Stored on the device; chats and bots are not translated.
/// `zh` stays the stored name for Simplified, so a choice saved before Traditional existed still reads it.
public enum AppLanguage: String, CaseIterable, Sendable, Identifiable, Hashable {
    case system
    case zh
    case zhHant = "zh-Hant"
    case en
    case ja
    case ko
    case fr
    case de
    case es

    public var catalogName: String { self == .zh ? "zh-Hans" : rawValue }

    public static func resolved(preference: AppLanguage, languages: [String] = Locale.preferredLanguages) -> AppLanguage {
        guard preference == .system else { return preference }
        let tag = (languages.first ?? "en").lowercased().replacingOccurrences(of: "_", with: "-")
        let base = String(tag.split(separator: "-").first ?? "en")
        if base == "zh" { return systemScript(languages: languages) == .hant ? .zhHant : .zh }
        return AppLanguage(rawValue: base).flatMap { $0 == .system ? nil : $0 } ?? .en
    }

    public var id: String { rawValue }

    public static let storageKey = "appLanguage"

    /// The App Group, so the widget extension reads the same choice the app writes.
    /// A choice saved before this was shared is copied across the first time the app looks.
    public static var defaults: UserDefaults {
        let shared = UserDefaults(suiteName: SharedStore.appGroup) ?? .standard
        if shared.string(forKey: storageKey) == nil,
           let existing = UserDefaults.standard.string(forKey: storageKey) {
            shared.set(existing, forKey: storageKey)
        }
        return shared
    }

    public static func preference(in store: UserDefaults = defaults) -> AppLanguage {
        _ = LanguageUpdates.shared.revision
        return AppLanguage(rawValue: store.string(forKey: storageKey) ?? "") ?? .system
    }

    public static func setPreference(_ language: AppLanguage, in store: UserDefaults = defaults) {
        store.set(language.rawValue, forKey: storageKey)
        LanguageUpdates.shared.changed()
    }

    public static func systemIsChinese(languages: [String] = Locale.preferredLanguages) -> Bool {
        systemScript(languages: languages) != nil
    }

    /// `zh-Hant`, `zh-TW`, `zh-HK` and `zh-MO` read Traditional. Other Chinese locales read Simplified.
    public static func systemScript(languages: [String] = Locale.preferredLanguages) -> ChineseScript? {
        guard let id = languages.first?.lowercased().replacingOccurrences(of: "_", with: "-") else { return nil }
        guard id.hasPrefix("zh") else { return nil }
        if id.contains("hant") || id.contains("-tw") || id.contains("-hk") || id.contains("-mo") { return .hant }
        return .hans
    }

    public static func usesChinese(preference: AppLanguage, systemIsChinese: Bool) -> Bool {
        switch preference {
        case .zh, .zhHant: true
        case .en, .ja, .ko, .fr, .de, .es: false
        case .system: systemIsChinese
        }
    }

    /// The locale SwiftUI should use so dates and string lookups follow the choice.
    public static func locale(preference: AppLanguage, systemIsChinese: Bool) -> Locale {
        switch preference {
        case .zh: return Locale(identifier: "zh-Hans")
        case .zhHant: return Locale(identifier: "zh-Hant")
        case .en, .ja, .ko, .fr, .de, .es: return Locale(identifier: preference.rawValue)
        case .system:
            return Locale(identifier: resolved(preference: .system).catalogName)
        }
    }

    /// What the language menu shows for this choice. Names of languages stay in that language.
    public static func menuTitle(_ language: AppLanguage, preference: AppLanguage, systemIsChinese: Bool) -> String {
        switch language {
        case .system: L10n.tr("Follow system", preference: preference, systemIsChinese: systemIsChinese)
        case .zh: "简体中文"
        case .zhHant: "繁體中文"
        case .en: "English"
        case .ja: "日本語"
        case .ko: "한국어"
        case .fr: "Français"
        case .de: "Deutsch"
        case .es: "Español"
        }
    }
}

/// English source strings, with a Simplified or Traditional table when the app language asks for it.
public enum L10n {
    private static let log = Logger(subsystem: "com.pokai.Codync", category: "L10n")

    private static let hans = load("zh-Hans")
    private static let hant = load("zh-Hant")
    private static let catalogs = Dictionary(uniqueKeysWithValues: [AppLanguage.ja, .ko, .fr, .de, .es].map { ($0, load($0.catalogName)) })

    /// A missing or malformed table is logged and left empty, so the interface stays in English.
    private static func load(_ name: String) -> [String: String] {
        guard let url = Bundle.module.url(forResource: name, withExtension: "json") else {
            log.error("\(name, privacy: .public) phrases are missing from the app bundle")
            return [:]
        }
        do {
            return try JSONDecoder().decode([String: String].self, from: Data(contentsOf: url))
        } catch {
            log.error("\(name, privacy: .public) phrases failed to load: \(error, privacy: .public)")
            return [:]
        }
    }

    public static func tr(
        _ key: String,
        preference: AppLanguage = AppLanguage.preference(),
        systemIsChinese: Bool = AppLanguage.systemIsChinese(),
        systemLanguages: [String] = Locale.preferredLanguages
    ) -> String {
        guard let tables = tables(preference: preference, systemIsChinese: systemIsChinese, systemLanguages: systemLanguages) else {
            return key
        }
        if let hit = phrase(key, primary: tables.primary, fallback: tables.fallback) { return hit }
        let trimmed = key.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed == key ? key : (phrase(trimmed, primary: tables.primary, fallback: tables.fallback) ?? key)
    }

    /// Simplified, Traditional, or neither. A missing Traditional line falls back to Simplified.
    private static func tables(
        preference: AppLanguage,
        systemIsChinese: Bool,
        systemLanguages: [String]
    ) -> (primary: [String: String], fallback: [String: String])? {
        switch preference {
        case .zh: return (hans, [:])
        case .zhHant: return (hant, hans)
        case .en: return nil
        case .ja, .ko, .fr, .de, .es: return (catalogs[preference] ?? [:], [:])
        case .system:
            if !systemIsChinese {
                let resolved = AppLanguage.resolved(preference: .system, languages: systemLanguages)
                return catalogs[resolved].map { ($0, [:]) }
            }
            return AppLanguage.systemScript(languages: systemLanguages) == .hant ? (hant, hans) : (hans, [:])
        }
    }

    /// One piece of a phrase key such as "Runs the official installer on {name}."
    private struct Piece {
        var literal: String?
        var token: String?
    }

    /// Phrase keys that already contain `{token}` slots, longest fixed text first.
    private static let templates: [(key: String, pieces: [Piece])] = {
        hans.compactMap { key, _ -> (Int, String, [Piece])? in
            guard let pieces = parseTemplate(key) else { return nil }
            let literal = pieces.reduce(0) { $0 + ($1.literal?.count ?? 0) }
            guard literal >= 6 else { return nil }
            return (literal, key, pieces)
        }
        .sorted { $0.0 == $1.0 ? $0.1.count > $1.1.count : $0.0 > $1.0 }
        .map { ($0.1, $0.2) }
    }()

    /// `{name}` and the same shape. A `{` that is not a slot makes the key unusable as a template.
    private static func parseTemplate(_ template: String) -> [Piece]? {
        var pieces: [Piece] = []
        var rest = template[...]
        while let open = rest.firstIndex(of: "{") {
            if open != rest.startIndex {
                pieces.append(Piece(literal: String(rest[..<open]), token: nil))
            }
            guard let close = rest[open...].firstIndex(of: "}") else { return nil }
            let token = String(rest[open...close])
            let name = token.dropFirst().dropLast()
            guard !name.isEmpty, name.allSatisfy({ $0.isLetter || $0.isNumber }), pieces.last?.token == nil else { return nil }
            pieces.append(Piece(literal: nil, token: token))
            rest = rest[rest.index(after: close)...]
        }
        if !rest.isEmpty { pieces.append(Piece(literal: String(rest), token: nil)) }
        return pieces.contains(where: { $0.token != nil }) ? pieces : nil
    }

    /// A sentence the host or a view already filled in, matched back to its `{token}` phrase.
    private static func phrase(_ key: String, primary: [String: String], fallback: [String: String]) -> String? {
        if let hit = primary[key] ?? fallback[key] { return hit }
        for template in templates {
            guard let phrase = primary[template.key] ?? fallback[template.key],
                  let filled = fill(key, pieces: template.pieces, phrase: phrase, primary: primary, fallback: fallback, depth: 0) else { continue }
            return filled
        }
        return nil
    }

    private static func fill(
        _ text: String,
        pieces: [Piece],
        phrase: String,
        primary: [String: String],
        fallback: [String: String],
        depth: Int
    ) -> String? {
        var pos = text.startIndex
        var caps: [String: String] = [:]
        for (index, piece) in pieces.enumerated() {
            if let literal = piece.literal {
                guard text[pos...].hasPrefix(literal) else { return nil }
                pos = text.index(pos, offsetBy: literal.count)
                continue
            }
            guard let token = piece.token else { return nil }
            let end: String.Index
            if let next = index + 1 < pieces.count ? pieces[index + 1].literal : nil {
                guard let found = text[pos...].range(of: next)?.lowerBound, found > pos else { return nil }
                end = found
            } else {
                guard pos < text.endIndex else { return nil }
                end = text.endIndex
            }
            caps[token] = String(text[pos..<end])
            pos = end
        }
        guard pos == text.endIndex else { return nil }
        var result = phrase
        for (token, value) in caps {
            let inner = primary[value] ?? fallback[value] ?? (depth == 0 ? nested(value, primary: primary, fallback: fallback) : nil) ?? value
            result = result.replacingOccurrences(of: token, with: inner)
        }
        return result
    }

    /// One more template pass, for a setup hint inside an error. It does not look inside its own slots.
    private static func nested(_ text: String, primary: [String: String], fallback: [String: String]) -> String? {
        for template in templates {
            guard let phrase = primary[template.key] ?? fallback[template.key],
                  let filled = fill(text, pieces: template.pieces, phrase: phrase, primary: primary, fallback: fallback, depth: 1) else { continue }
            return filled
        }
        return nil
    }

    /// "resets 3h 20m" / "resets in 2d 6h". Compact times become "3小时20分后重置".
    public static func resetLine(_ raw: String) -> String {
        guard let time = resetRemainder(raw) else { return tr(raw) }
        let preference = AppLanguage.preference()
        guard AppLanguage.usesChinese(preference: preference, systemIsChinese: AppLanguage.systemIsChinese()) else {
            if AppLanguage.resolved(preference: preference) == .en { return raw }
            return compactTime(time) != nil
                ? tr("resets in {time}").replacingOccurrences(of: "{time}", with: time)
                : "\(tr("resets")) \(time)"
        }
        if let spoken = compactTime(time) { return spoken + tr("after reset") }
        return "\(tr("resets")) \(time)"
    }

    private static func resetRemainder(_ raw: String) -> String? {
        if raw.hasPrefix("resets in ") { return String(raw.dropFirst("resets in ".count)) }
        if raw.hasPrefix("resets ") { return String(raw.dropFirst("resets ".count)) }
        return nil
    }

    /// "3h 20m", "2d 6h", "45m". A clock time such as "Sep 26 at 12pm" stays nil.
    private static func compactTime(_ time: String) -> String? {
        var spoken = ""
        var matched = false
        for token in time.split(separator: " ") {
            guard token.count >= 2, let unit = token.last, let n = Int(token.dropLast()) else { return nil }
            let word: String
            switch unit {
            case "d": word = tr("day unit")
            case "h": word = tr("hour unit")
            case "m": word = tr("minute unit")
            default: return nil
            }
            matched = true
            if n == 0 { continue }
            spoken += "\(n)\(word)"
        }
        guard matched else { return nil }
        return spoken.isEmpty ? "0\(tr("minute unit"))" : spoken
    }
}
