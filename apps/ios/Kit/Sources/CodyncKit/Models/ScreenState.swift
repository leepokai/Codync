import Foundation

/// The computer's remote screen: whether phones can view/control it, and who's in control.
public struct ScreenState: Codable, Equatable, Sendable {
    public var enabled = false
    /// The screen helper is running.
    public var connected = false
    public var platform = ""
    /// App name shown in the computer’s system permission settings.
    public var permissionApp: String?
    /// Screen recording is permitted.
    public var capture = false
    /// Input injection is permitted.
    public var input = false
    public var displays: [ScreenDisplay] = []
    /// A phone took over: bots may only look.
    public var userControl = false
    /// The bot using the computer right now.
    public var agentBot: String?
    /// Bots with computer use can act here (Windows: always; elsewhere Remote screen is on).
    public var computerUse = false
    public var viewers = 0

    public init() {}

    public var available: Bool { enabled && connected && capture }
    public var controlReady: Bool { platform == "windows" ? computerUse : available && input }
    public var mainDisplay: ScreenDisplay? { displays.first(where: \.main) ?? displays.first }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try c.decodeIfPresent(Bool.self, forKey: .enabled) ?? false
        connected = try c.decodeIfPresent(Bool.self, forKey: .connected) ?? false
        platform = try c.decodeIfPresent(String.self, forKey: .platform) ?? ""
        permissionApp = try c.decodeIfPresent(String.self, forKey: .permissionApp)
        capture = try c.decodeIfPresent(Bool.self, forKey: .capture) ?? false
        input = try c.decodeIfPresent(Bool.self, forKey: .input) ?? false
        displays = try c.decodeIfPresent([ScreenDisplay].self, forKey: .displays) ?? []
        userControl = try c.decodeIfPresent(Bool.self, forKey: .userControl) ?? false
        agentBot = try c.decodeIfPresent(String.self, forKey: .agentBot)
        computerUse = try c.decodeIfPresent(Bool.self, forKey: .computerUse) ?? enabled
        viewers = try c.decodeIfPresent(Int.self, forKey: .viewers) ?? 0
    }
}

public struct ScreenDisplay: Codable, Hashable, Identifiable, Sendable {
    public var id: UInt32
    public var name: String
    /// Size in points: the coordinate space of screen input.
    public var width: Double
    public var height: Double
    public var main: Bool
}

extension ScreenState {
    /// Remote clients explain setup; only the computer itself can request OS permissions.
    public var accessGuidance: String {
        if platform == "windows" {
            return "Remote screen viewing is not available on Windows yet. Bots can still use apps on that computer."
        }
        let setup = "On the computer, open Codync → Settings → Computer access. Permissions are set separately on each computer."
        if !enabled { return "Remote screen is off. \(setup)" }
        if !connected { return "Codync Screen is not running. \(setup)" }
        if !capture || (platform == "linux" && !input) {
            if platform == "linux" { return "Choose a screen and allow control in the system sharing dialog. \(setup)" }
            return "Allow screen recording for \(permissionApp ?? "Codync Screen"). \(setup)"
        }
        if !input { return "Allow computer control for \(permissionApp ?? "Codync Screen"). macOS may call this Device Control and Data Access or Accessibility. \(setup)" }
        return "Computer access is ready."
    }
}
