import CodyncKit
import SwiftUI

/// A device preference: while the computer's bots run more than one agent, the roster marks each bot with its
/// agent's logo. On by default.
public enum AgentLogos {
    public static let storageKey = "showAgentLogos"
}

/// The agent's logo on a background-colored disc, for an avatar's bottom-left corner.
struct AgentBadge: View {
    let registry: String
    let size: CGFloat

    var body: some View {
        AgentIcon(registry: registry, size: size * 0.7)
            .frame(width: size, height: size)
            .background(Palette.background, in: Circle())
    }
}

/// Settings › Appearance: whether the roster shows agent logos on this device.
public struct AgentLogosToggle: View {
    @AppStorage(AgentLogos.storageKey) private var on = true

    public init() {}

    public var body: some View {
        SwitchRow("Show agent logos", detail: "Marks each bot with its agent's logo when your bots use more than one agent.", isOn: $on)
    }
}
