import CodyncKit
import CodyncUI
import SwiftUI

struct EmptyRoster: View {
    let canCreate: Bool
    let hasComputer: Bool
    let create: () -> Void
    let showComputers: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            CharacterAvatar(shape: "cloud", color: "green", size: 72)
            Text(L10n.tr("No bots yet")).font(.title3.weight(.semibold)).foregroundStyle(Palette.text)
            Text(L10n.tr(hasComputer
                 ? "Create a bot for each kind of work — a reviewer, a fixer, a docs writer — and point it at a project."
                 : "Ask one of your account's computers for access, or pair one with its code."))
                .font(.subheadline)
                .foregroundStyle(Palette.secondary)
                .multilineTextAlignment(.center)
            if canCreate {
                Button(L10n.tr("Create your first bot"), action: create)
                    .buttonStyle(.primary)
            } else if !hasComputer {
                Button(L10n.tr("Computers"), action: showComputers)
                    .buttonStyle(.primary)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 40)
    }
}
