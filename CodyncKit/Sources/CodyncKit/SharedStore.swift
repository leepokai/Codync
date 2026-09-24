import Foundation

/// Values shared between the iOS app and its widgets through the App Group.
public enum SharedStore {
    public static let appGroup = "group.com.pokai.Codync"
    /// Codync push relay (relay/ in this repo).
    public static let relayURL = "https://codync-relay.kevin2005ha.workers.dev"

    private static var defaults: UserDefaults { UserDefaults(suiteName: appGroup) ?? .standard }

    public static var pairing: Pairing? {
        get { defaults.data(forKey: "pairing").flatMap { try? JSONDecoder().decode(Pairing.self, from: $0) } }
        set { defaults.set(newValue.flatMap { try? JSONEncoder().encode($0) }, forKey: "pairing") }
    }

    /// Last address that answered, tried first next time.
    public static var preferredURL: String? {
        get { defaults.string(forKey: "preferredURL") }
        set { defaults.set(newValue, forKey: "preferredURL") }
    }

    public static var usage: Usage? {
        get { defaults.data(forKey: "usage").flatMap { try? JSONDecoder().decode(Usage.self, from: $0) } }
        set { defaults.set(newValue.flatMap { try? JSONEncoder().encode($0) }, forKey: "usage") }
    }

    /// Pairing with the last working address moved to the front.
    public static var orderedPairing: Pairing? {
        guard var p = pairing else { return nil }
        if let preferred = preferredURL, let i = p.urls.firstIndex(of: preferred) {
            p.urls.insert(p.urls.remove(at: i), at: 0)
        }
        return p
    }
}
