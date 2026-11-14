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
