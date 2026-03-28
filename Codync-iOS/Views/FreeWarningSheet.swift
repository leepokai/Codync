import SwiftUI

struct FreeWarningSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    @State private var showPaywall = false

    var body: some View {
        ZStack {
            theme.background.ignoresSafeArea()

            VStack(spacing: 0) {
                // Close button
                HStack {
                    Spacer()
                    Button { dismiss() } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 15, weight: .medium))
                            .foregroundStyle(theme.tertiaryText)
                            .frame(width: 32, height: 32)
                            .background(theme.cardBackground, in: Circle())
                    }
                }
                .padding(.horizontal, 20)
                .padding(.top, 16)

                Spacer()

                VStack(spacing: 20) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .font(.system(size: 40))
                        .foregroundStyle(theme.secondaryText)

                    Text("Free Plan Limitations")
                        .font(.title2.bold())
                        .foregroundStyle(theme.primaryText)

                    VStack(alignment: .leading, spacing: 12) {
                        warningRow(
                            icon: "clock.arrow.circlepath",
                            text: "Updates may be delayed — free plan relies on iCloud polling instead of instant push notifications"
                        )
                        warningRow(
                            icon: "livephoto.slash",
                            text: "Live Activities may not update in the background when the app is closed"
                        )
                        warningRow(
                            icon: "bell.slash",
                            text: "Session completion alerts may arrive late or not at all"
                        )
                    }
                    .padding(.horizontal, 8)
                }
                .padding(.horizontal, 24)

                Spacer()

                Button {
                    showPaywall = true
                } label: {
                    Text("Upgrade to Pro")
                        .font(.headline)
                        .foregroundStyle(theme.isDark ? .black : .white)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 14)
                        .background(theme.primaryText, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                }
                .padding(.horizontal, 24)
                .padding(.bottom, 8)

                Button { dismiss() } label: {
                    Text("Maybe Later")
                        .font(.subheadline)
                        .foregroundStyle(theme.tertiaryText)
                }
                .padding(.bottom, 24)
            }
        }
        .sheet(isPresented: $showPaywall) {
            CodyncPaywallView()
        }
    }

    @ViewBuilder
    private func warningRow(icon: String, text: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: icon)
                .font(.system(size: 16, weight: .medium))
                .foregroundStyle(theme.secondaryText)
                .frame(width: 24)
            Text(text)
                .font(.subheadline)
                .foregroundStyle(theme.secondaryText)
                .fixedSize(horizontal: false, vertical: true)
        }
    }
}
