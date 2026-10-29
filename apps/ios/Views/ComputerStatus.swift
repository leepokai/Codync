import CodyncKit
import CodyncUI
import SwiftUI

extension BotStore {
    /// One short line for a computer's connection, used next to its name.
    var statusText: String {
        if isUpdating { return "Updating…" }
        return switch connection {
        case .online where mismatch != nil: "Needs update"
        case .online: "Online"
        case .connecting: "Connecting…"
        case let .computerOffline(lastSeen): lastSeen.map { "Offline · seen \(RelativeTime.day($0))" } ?? "Offline"
        case .offline: "Can't reach"
        case let .unauthorized(message): message
        case .unpaired: "Not paired"
        }
    }
}

extension AccountStore {
    /// A computer's status line; one that's offline while its new identity is around says so.
    func statusText(_ store: BotStore) -> String {
        if case .unauthorized = store.connection { return store.statusText }
        return isStale(store.computer.id) ? "Older copy of this computer · \(store.statusText)" : store.statusText
    }
}

/// How the phone reaches a computer right now: a subtle icon, direct Wi-Fi/LAN or the encrypted relay.
struct RouteIcon: View {
    let route: HostRoute?
    var cloud: URL?

    var body: some View {
        switch route {
        case .direct:
            Image(systemName: "wifi").accessibilityLabel("Wi-Fi or Tailscale")
        case .relay:
            Image(systemName: "cloud").accessibilityLabel(L10n.tr(RelayCopy.through(cloud: cloud)))
        case nil:
            EmptyView()
        }
    }
}
