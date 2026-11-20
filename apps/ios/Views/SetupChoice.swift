import CodyncKit
import CodyncUI
import SwiftUI

/// Another way through setup, as a card holding one row: its symbol, what it does, and a chevron.
struct SetupChoiceButton: View {
    let systemImage: String
    let title: String
    let detail: String
    let action: () -> Void

    var body: some View {
        CardSection {
            Button(action: action) {
                HStack(spacing: 12) {
                    Image(systemName: systemImage)
                        .font(.title3)
                        .foregroundStyle(Palette.text)
                        .frame(width: 40, height: 40)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(L10n.tr(title)).font(.body.weight(.semibold)).foregroundStyle(Palette.text)
                        Text(L10n.tr(detail)).font(.subheadline).foregroundStyle(Palette.secondary)
                    }
                    Spacer(minLength: 8)
                    Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(Palette.tertiary)
                }
                .padding(.vertical, 2)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }
}

/// First run, signed in with Codync Cloud open to the account: start a cloud computer instead of installing Codync
/// (New cloud computer, docs/plans/codync-cloud.md).
struct StartCloudButton: View {
    let action: () -> Void

    var body: some View {
        SetupChoiceButton(systemImage: "cloud", title: "Start a cloud computer",
                          detail: "No computer? Run one on your own cloud provider account.", action: action)
            .accessibilityIdentifier("startCloudComputer")
    }
}
