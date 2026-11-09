import CodyncKit
import CodyncUI
import SwiftUI

/// A computer in the account that this iPhone can't use yet.
struct AccountComputerRow: View {
    let computer: CloudComputer
    let ticket: AccessTicket?
    let ask: () -> Void
    /// Takes back an earlier install's approval first; without it, asking offers to replace that approval.
    var revoke: (() -> Void)?

    var body: some View {
        HStack(spacing: 12) {
            ComputerBadge(Computer(id: computer.computerId, name: computer.name, signKey: computer.signKey, device: computer.device), size: 40)
                .opacity(0.5)
            VStack(alignment: .leading, spacing: 2) {
                Text(computer.name).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                Text(L10n.tr(detail)).font(.subheadline).foregroundStyle(Palette.secondary).lineLimit(2)
                if let hosting = computer.cloud { CloudHostingLine(hosting) }
            }
            Spacer()
            if ticket != nil {
                Button(L10n.tr("Code"), action: ask).buttonStyle(.secondary)
            } else if computer.isGranted, let revoke {
                Button(L10n.tr("Revoke"), role: .destructive, action: revoke).buttonStyle(.secondary)
            } else {
                Button(L10n.tr("Ask for access"), action: ask).buttonStyle(.secondary)
            }
        }
        .padding(.vertical, 2)
    }

    private var detail: String {
        let online = computer.isOnline ? "Online" : "Offline"
        // A cloud computer approves this iPhone by itself: there's no code to check.
        if let ticket { return computer.cloud == nil ? "Waiting for your OK on the computer · \(ticket.code)" : "Connecting…" }
        return switch computer.access {
        case "granted" where revoke != nil: "Approved for an earlier install of Codync. Revoke it, then ask again."
        case "pending": "\(online) · A request is waiting"
        default: !computer.isOnline && computer.cloud?.state.isAsleep == true ? "Sleeping · wakes when you message" : "\(online) · In your account"
        }
    }
}

/// An older account identity stays visible; only account management can remove it.
struct OlderCopyRow: View {
    let computer: CloudComputer

    var body: some View {
        HStack(spacing: 12) {
            ComputerBadge(Computer(id: computer.computerId, name: computer.name, signKey: computer.signKey, device: computer.device), size: 40)
                .opacity(0.35)
            VStack(alignment: .leading, spacing: 2) {
                Text(computer.name).font(.body.weight(.semibold)).foregroundStyle(Palette.secondary)
                Text(L10n.tr(detail)).font(.subheadline).foregroundStyle(Palette.tertiary).lineLimit(2)
            }
            Spacer()
        }
        .padding(.vertical, 2)
    }

    private var detail: String {
        let seen = computer.lastSeenAt.map { " · last seen \(RelativeTime.day(Date(milliseconds: $0)))" } ?? ""
        return "Older copy of this computer\(seen)"
    }
}
