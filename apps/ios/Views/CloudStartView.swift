import CodyncKit
import CodyncUI
import SwiftUI

/// First run, while the cloud computer started there gets ready: preparing it, then letting this iPhone in. The tabs
/// follow once it's reached; Back returns to the setup pages and Skip goes on without waiting, and either way it turns up
/// in Computers when it's ready.
struct CloudStartView<Leading: View>: View {
    let machine: CloudMachine
    /// Back to the pages it was started from, whatever its stage: setup stops waiting for it.
    let back: () -> Void
    let skip: () -> Void
    @ViewBuilder var leading: Leading

    var body: some View {
        VStack(spacing: 0) {
            ScreenHeader { leading } title: { EmptyView() } trailing: { EmptyView() }

            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    // A bot on a cloud: that's where it lives now.
                    ZStack {
                        Image(systemName: "cloud")
                            .font(.system(size: 150, weight: .ultraLight))
                            .foregroundStyle(Palette.tertiary)
                        CharacterAvatar(shape: "blob", color: "blue", size: 52, mood: machine.start == .failed ? .needsInput : .working)
                            .offset(y: 10)
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.top, 8)

                    VStack(alignment: .leading, spacing: 10) {
                        Text(L10n.tr("Starting your cloud computer")).pageTitle()
                        Text(verbatim: "\(machine.name) · \(CloudHosting.runsOn(providerName: machine.hosting.providerName, region: machine.hosting.region))")
                            .font(.body)
                            .foregroundStyle(Palette.secondary)
                    }

                    VStack(alignment: .leading, spacing: 12) {
                        status
                        Button(action: back) {
                            Text(L10n.tr("Back")).font(.headline).frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.secondary)
                    }
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 16)
            }

            SkipSetupButton(action: skip)
        }
        .background(Palette.background)
        .animation(Motion.fade, value: machine.start)
        .tracked(.onboardingStepViewed(.cloud))
    }

    @ViewBuilder
    private var status: some View {
        switch machine.start {
        case .preparing, .connecting:
            VStack(alignment: .leading, spacing: 10) {
                HStack(spacing: 10) {
                    Spinner(size: 18)
                    Text(L10n.tr(machine.start == .preparing ? "Preparing your cloud computer…" : "Connecting…"))
                        .font(.body)
                        .foregroundStyle(Palette.text)
                        .contentTransition(.opacity)
                }
                Text(L10n.tr("About a minute. You can skip: it shows up on its own when it's ready."))
                    .font(.footnote)
                    .foregroundStyle(Palette.secondary)
            }
            .transition(.opacity)
        case .failed:
            Text(L10n.tr("Cloud computer failed")).font(.body).foregroundStyle(Palette.danger)
                .transition(.opacity)
        }
    }
}
