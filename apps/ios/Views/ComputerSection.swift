import CodyncKit
import CodyncUI
import SwiftUI

/// A computer's heading above its bots: badge, name and connection. Tap folds its bots away;
/// touch and hold, then drag, moves the computer (on this device).
struct ComputerSection: View {
    let store: BotStore
    let empty: Bool
    let collapsed: Bool
    let toggle: () -> Void
    let move: (ComputerID, Int) -> Void
    /// This computer's own Extensions; nil while it can't be reached.
    let extensions: (() -> Void)?

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            heading
            if empty && !collapsed {
                Text(L10n.tr("No bots yet"))
                    .font(.footnote)
                    .foregroundStyle(Palette.tertiary)
                    .padding(.horizontal, 8)
                    .padding(.bottom, 4)
                    .transition(.opacity)
            }
        }
        .padding(.top, 14)
        .padding(.bottom, 2)
        .padding(.horizontal, -8)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits([.isHeader, .isButton])
        .accessibilityLabel("\(store.hostName), \(store.statusText)")
        .accessibilityValue(collapsed ? "Collapsed" : "Expanded")
        .accessibilityHint("Double-tap to fold or expand. Touch and hold, then drag to reorder on this device.")
        .accessibilityAction(named: collapsed ? "Expand" : "Collapse", toggle)
        .accessibilityAction(.default) { toggle() }
        .accessibilityActions {
            Button(L10n.tr("Move up")) { move(store.computer.id, -1) }
            Button(L10n.tr("Move down")) { move(store.computer.id, 1) }
            if let extensions { Button(L10n.tr("Extensions"), action: extensions) }
        }
    }

    private var heading: some View {
        HStack(spacing: 6) {
            ComputerBadge(store.computer, size: 18)
            Text(store.hostName)
                .font(.footnote.weight(.semibold))
                .foregroundStyle(Palette.secondary)
                .lineLimit(1)
            // The connection sits at the trailing edge, before the heading's buttons.
            Spacer(minLength: 8)
            Text(L10n.tr(store.statusText))
                .font(.footnote)
                .foregroundStyle(store.isOffline && !store.isUpdating ? Palette.warning : Palette.tertiary)
                .lineLimit(1)
                .contentTransition(.opacity)
            if store.connection == .online {
                RouteIcon(route: store.hostRoute, cloud: store.computer.cloud).font(.caption2).foregroundStyle(Palette.tertiary)
            }
            Image(systemName: "chevron.down")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.tertiary)
                .rotationEffect(.degrees(collapsed ? -90 : 0))
                .accessibilityHidden(true)
            if let extensions {
                // After the chevron, as on the desktop; sized like it so the heading stays one light line.
                Button(action: extensions) {
                    Image(systemName: "square.grid.2x2")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Palette.secondary)
                        .frame(width: 28, height: 22)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.borderless)
                .accessibilityLabel(L10n.tr("Extensions"))
                .help(L10n.tr("Extensions"))
            }
        }
        .padding(.vertical, 6)
        .padding(.horizontal, 8)
        .contentShape(Rectangle())
    }
}
