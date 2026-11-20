import CodyncKit
import CodyncUI
import SwiftUI

/// First run, signed in to an account that already has computers: ask one of them for access, pair a new one (a VPS,
/// say), or start a cloud computer. Nothing is asked until a tap. An account with none goes straight to `PairingView`.
struct ChooseComputerView<Leading: View>: View {
    let computers: [CloudComputer]
    let ask: (CloudComputer) -> Void
    let pairNew: () -> Void
    let skip: () -> Void
    /// Opens New cloud computer; nil while Codync Cloud isn't open to this account.
    var startCloud: (() -> Void)?
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
                    SetupChoiceButton(systemImage: "plus", title: "Pair a new computer", detail: "Install Codync on a VPS or another machine.",
                                      action: pairNew)
                    if let startCloud { StartCloudButton(action: startCloud) }
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 16)
            }

            SkipSetupButton(action: skip)
        }
        .background(Palette.background)
        .tracked(.onboardingStepViewed(.choose))
    }
}
