import CodyncKit
import CodyncUI
import SwiftUI

/// How this iPhone reaches the computer: the short name on the row, the whole route in the menu. With no relay
/// (a computer in no account) there's nothing to choose.
struct ComputerConnectionRow: View {
    let store: BotStore
    @Environment(AccountStore.self) private var accounts

    var body: some View {
        if store.computer.cloud == nil {
            ConnectionRowLabel(route: .directOnly, cloud: nil, showsChevron: false)
        } else {
            DropdownMenu {
                ConnectionRoute.allCases.map { route in
                    MenuItem(RelayCopy.routeTitle(route, cloud: store.computer.cloud), icon: route.icon, selected: route == preference) {
                        accounts.setRoute(store.computer.id, route)
                    }
                }
            } label: {
                ConnectionRowLabel(route: preference, cloud: store.computer.cloud, showsChevron: true)
                    .contentShape(Rectangle())
            }
        }
    }

    private var preference: ConnectionRoute { store.computer.route ?? .automatic }
}

private struct ConnectionRowLabel: View {
    let route: ConnectionRoute
    let cloud: URL?
    let showsChevron: Bool

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: route.icon).font(.system(size: 17)).frame(width: 26)
            Text(L10n.tr("Connection"))
            Spacer(minLength: 8)
            Text(L10n.tr(RelayCopy.routeShortTitle(route, cloud: cloud)))
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
            if showsChevron {
                Image(systemName: "chevron.up.chevron.down").font(.caption.weight(.semibold)).foregroundStyle(Palette.tertiary)
            }
        }
        .foregroundStyle(Palette.text)
    }
}

/// A computer in no account: adding it lets this iPhone reach it from anywhere. The computer confirms.
struct AddToAccountRow: View {
    let store: BotStore
    @Environment(AccountStore.self) private var accounts

    var body: some View {
        if let check = accounts.addToAccountCheck(store.computer.id) {
            HStack(spacing: 12) {
                Spinner(size: 17).frame(width: 26)
                Text(L10n.tr("Confirm on the computer. It shows {code}.").replacingOccurrences(of: "{code}", with: check))
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        } else if accounts.canAddToAccount(store) {
            Button {
                accounts.addToAccount(store.computer.id, deviceName: UIDevice.current.name, platform: "ios")
            } label: {
                HStack(spacing: 12) {
                    Image(systemName: "person.crop.circle.badge.plus").font(.system(size: 17)).frame(width: 26)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(L10n.tr("Add to account"))
                        Text(L10n.tr("Reach this computer from anywhere, not only over Wi-Fi or Tailscale."))
                            .font(.footnote)
                            .foregroundStyle(Palette.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer(minLength: 0)
                }
                .foregroundStyle(Palette.text)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }
}

private extension ConnectionRoute {
    var icon: String {
        switch self {
        case .automatic: "arrow.triangle.branch"
        case .cloudflareFirst: "cloud"
        case .directOnly: "wifi"
        }
    }
}
