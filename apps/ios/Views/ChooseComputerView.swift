import CodyncKit
import CodyncUI
import SwiftUI

/// First run, signed in to an account that already has computers: ask one of them for access, or pair a
/// new one (a VPS, say). Nothing is asked until a tap. An account with none goes straight to `PairingView`.
struct ChooseComputerView<Leading: View>: View {
    let computers: [CloudComputer]
    let ask: (CloudComputer) -> Void
    let pairNew: () -> Void
    let skip: () -> Void
    @ViewBuilder var leading: Leading
    @Environment(AccountStore.self) private var accounts

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader { leading } title: { EmptyView() } trailing: { EmptyView() }

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    VStack(alignment: .leading, spacing: 10) {
                        Text(L10n.tr("Connect a computer")).pageTitle()
                        Text(L10n.tr("Ask a computer in your account for access, or pair a new one, like a VPS."))
                            .font(.body)
                            .foregroundStyle(Palette.secondary)
                    }
                    CardSection("In your account") {
                        ForEach(computers) { computer in
                            AccountComputerRow(computer: computer, ticket: accounts.pendingAccess[computer.computerId]) { ask(computer) }
                        }
                    }
                    CardSection {
                        Button(action: pairNew) { pairNewLabel }
                            .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 16)
            }

            SkipSetupButton(action: skip)
        }
        .background(Palette.background)
        .tracked(.onboardingStepViewed(.choose))
    }

    private var pairNewLabel: some View {
        HStack(spacing: 12) {
            Image(systemName: "plus")
                .font(.title3)
                .foregroundStyle(Palette.text)
                .frame(width: 40, height: 40)
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.tr("Pair a new computer")).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                Text(L10n.tr("Install Codync on a VPS or another machine.")).font(.subheadline).foregroundStyle(Palette.secondary)
            }
            Spacer(minLength: 8)
            Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(Palette.tertiary)
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
    }
}
