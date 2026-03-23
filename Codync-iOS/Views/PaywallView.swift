import SwiftUI
import RevenueCat

struct CodyncPaywallView: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    @State private var offering: Offering?
    @State private var isPurchasing = false
    @State private var errorMessage: String?

    var body: some View {
        ZStack {
            theme.background.ignoresSafeArea()

            ScrollView {
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

                    Spacer().frame(height: 40)

                    // Icon
                    Image("CodyncIcon")
                        .resizable()
                        .aspectRatio(contentMode: .fit)
                        .frame(width: 72, height: 72)
                        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))

                    Spacer().frame(height: 20)

                    // Title
                    Text("Codync Pro")
                        .font(.system(size: 28, weight: .bold))
                        .foregroundStyle(theme.primaryText)

                    Spacer().frame(height: 8)

                    Text("Always-on Live Activity")
                        .font(.system(size: 15))
                        .foregroundStyle(theme.secondaryText)

                    Spacer().frame(height: 36)

                    // Features
                    VStack(spacing: 0) {
                        featureRow(
                            icon: "dot.radiowaves.up.forward",
                            title: "Background Push Updates",
                            subtitle: "Live Activity stays updated even when the app is closed"
                        )
                        Divider().overlay(theme.separator)
                        featureRow(
                            icon: "lock.iphone",
                            title: "Dynamic Island Always-on",
                            subtitle: "Session status persists on your Lock Screen"
                        )
                        Divider().overlay(theme.separator)
                        featureRow(
                            icon: "bolt.fill",
                            title: "Instant APNs Delivery",
                            subtitle: "Direct push via Worker relay, no polling delay"
                        )
                    }
                    .background(
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .fill(theme.cardBackground)
                            .overlay(
                                RoundedRectangle(cornerRadius: 14, style: .continuous)
                                    .stroke(theme.border, lineWidth: 1)
                            )
                    )
                    .padding(.horizontal, 20)

                    Spacer().frame(height: 32)

                    // Price
                    if let pkg = offering?.monthly {
                        Text(pkg.localizedPriceString + " / month")
                            .font(.system(size: 17, weight: .semibold))
                            .foregroundStyle(theme.primaryText)
                    } else {
                        Text("$0.99 / month")
                            .font(.system(size: 17, weight: .semibold))
                            .foregroundStyle(theme.primaryText)
                    }

                    Spacer().frame(height: 16)

                    // Subscribe button
                    Button {
                        Task { await purchase() }
                    } label: {
                        Group {
                            if isPurchasing {
                                ProgressView()
                                    .tint(theme.background)
                            } else {
                                Text("Subscribe")
                                    .font(.system(size: 16, weight: .semibold))
                            }
                        }
                        .frame(maxWidth: .infinity)
                        .frame(height: 50)
                        .background(theme.primaryText, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                        .foregroundStyle(theme.background)
                    }
                    .disabled(isPurchasing)
                    .padding(.horizontal, 20)

                    Spacer().frame(height: 12)

                    // Restore
                    Button {
                        Task { await restore() }
                    } label: {
                        Text("Restore Purchases")
                            .font(.system(size: 14))
                            .foregroundStyle(theme.tertiaryText)
                    }

                    if let error = errorMessage {
                        Spacer().frame(height: 12)
                        Text(error)
                            .font(.system(size: 13))
                            .foregroundStyle(.red.opacity(0.8))
                            .multilineTextAlignment(.center)
                            .padding(.horizontal, 20)
                    }

                    Spacer().frame(height: 16)

                    // Legal
                    Text("Payment will be charged to your Apple ID. Subscription automatically renews unless cancelled at least 24 hours before the end of the current period.")
                        .font(.system(size: 11))
                        .foregroundStyle(theme.tertiaryText)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 32)

                    Spacer().frame(height: 32)
                }
            }
        }
        .task {
            do {
                let offerings = try await Purchases.shared.offerings()
                offering = offerings.current
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    @ViewBuilder
    private func featureRow(icon: String, title: String, subtitle: String) -> some View {
        HStack(spacing: 14) {
            Image(systemName: icon)
                .font(.system(size: 16))
                .foregroundStyle(theme.primaryText)
                .frame(width: 32, height: 32)
                .background(theme.glassBackground, in: RoundedRectangle(cornerRadius: 8, style: .continuous))

            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(theme.primaryText)
                Text(subtitle)
                    .font(.system(size: 12))
                    .foregroundStyle(theme.secondaryText)
            }
            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
    }

    private func purchase() async {
        guard let pkg = offering?.monthly ?? offering?.availablePackages.first else {
            errorMessage = "No product available"
            return
        }
        isPurchasing = true
        errorMessage = nil
        do {
            let result = try await Purchases.shared.purchase(package: pkg)
            if !result.userCancelled {
                await PremiumManager.shared.refreshStatus()
                // Pro requires notifications for APNs push
                let center = UNUserNotificationCenter.current()
                let settings = await center.notificationSettings()
                if settings.authorizationStatus == .notDetermined {
                    _ = try? await center.requestAuthorization(options: [.alert, .sound, .badge])
                } else if settings.authorizationStatus == .denied {
                    if let url = URL(string: UIApplication.openNotificationSettingsURLString) {
                        await UIApplication.shared.open(url)
                    }
                }
                dismiss()
            }
        } catch {
            errorMessage = error.localizedDescription
        }
        isPurchasing = false
    }

    private func restore() async {
        isPurchasing = true
        errorMessage = nil
        do {
            try await PremiumManager.shared.restorePurchases()
            if PremiumManager.shared.isPro {
                dismiss()
            } else {
                errorMessage = "No active subscription found"
            }
        } catch {
            errorMessage = error.localizedDescription
        }
        isPurchasing = false
    }
}
