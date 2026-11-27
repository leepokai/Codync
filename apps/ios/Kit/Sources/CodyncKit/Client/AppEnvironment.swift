import Foundation

/// Read from the containing app or extension, so the shared package follows its build configuration.
public enum AppEnvironment: String, Sendable {
    case main
    case dev

    public static let current = AppEnvironment(
        rawValue: Bundle.main.object(forInfoDictionaryKey: "CodyncEnvironment") as? String ?? "main"
    ) ?? .main

    /// The containing app's Info.plist (`CodyncAppGroup`) wins, so a build signed by
    /// another team can use the group that team owns. The fixed ids remain the fallback.
    public var appGroup: String {
        if let group = Bundle.main.object(forInfoDictionaryKey: "CodyncAppGroup") as? String,
           group.hasPrefix("group."), !group.contains("$(") {
            return group
        }
        return self == .dev ? "group.com.pokai.Codync.dev" : "group.com.pokai.Codync"
    }
    public var urlScheme: String { self == .dev ? "codync-dev" : "codync" }

    public func link(_ destination: String) -> URL {
        // All callers supply a fixed app route.
        URL(string: "\(urlScheme)://\(destination)")!
    }
}
