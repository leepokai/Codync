/// A device preference; it does not change the host or another client's appearance.
public enum Appearance: String, CaseIterable, Sendable {
    case system, light, dark

    public static let storageKey = "appearance"

    public var label: String {
        switch self {
        case .system: "System"
        case .light: "Light"
        case .dark: "Dark"
        }
    }
}

/// How a bot's character is drawn: plush (shaded dots under a light) or flat (even dots, no light).
/// A device preference like the theme; the App Group store carries it to the widgets too.
public enum AvatarStyle: String, CaseIterable, Sendable {
    case plush, flat

    public static let storageKey = "avatarStyle"

    public var label: String {
        switch self {
        case .plush: L10n.tr("Plush")
        case .flat: L10n.tr("Flat")
        }
    }
}

public extension SharedStore {
    /// The widgets draw the same style as the app, so both read the App Group store.
    static var avatarStyle: AvatarStyle {
        get { UserDefaults(suiteName: appGroup)?.string(forKey: AvatarStyle.storageKey).flatMap(AvatarStyle.init(rawValue:)) ?? .plush }
        set { UserDefaults(suiteName: appGroup)?.set(newValue.rawValue, forKey: AvatarStyle.storageKey) }
    }
}
