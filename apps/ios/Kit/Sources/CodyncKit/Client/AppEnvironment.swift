import Foundation

/// Read from the containing app or extension, so the shared package follows its build configuration.
public enum AppEnvironment: String, Sendable {
    case main
    case dev

    public static let current = AppEnvironment(
        rawValue: Bundle.main.object(forInfoDictionaryKey: "CodyncEnvironment") as? String ?? "main"
    ) ?? .main

    public var appGroup: String { self == .dev ? "group.com.pokai.Codync.dev" : "group.com.pokai.Codync" }
    public var urlScheme: String { self == .dev ? "codync-dev" : "codync" }
    public var appName: String { self == .dev ? "Codync Dev" : "Codync" }

    public func link(_ destination: String) -> URL {
        // All callers supply a fixed app route.
        URL(string: "\(urlScheme)://\(destination)")!
    }
}
