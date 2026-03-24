import SwiftUI
import RevenueCatUI

struct SettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @AppStorage("codync_darkMode") private var isDarkMode = true
    @State private var showPaywall = false

    private var theme: CodyncTheme { CodyncTheme(isDark: isDarkMode) }

    var body: some View {
        NavigationStack {
            List {
                // MARK: - Subscription
                Section {
                    subscriptionRow
                } header: {
                    Text("Subscription").foregroundStyle(.secondary)
                }

                // MARK: - Appearance
                Section {
                    Toggle(isOn: $isDarkMode) {
                        Label("Dark Mode", systemImage: "moon.fill")
                    }
                    .tint(.primary)
                } header: {
                    Text("Appearance").foregroundStyle(.secondary)
                }

                // MARK: - Notifications
                Section {
                    Button {
                        if let url = URL(string: UIApplication.openNotificationSettingsURLString) {
                            UIApplication.shared.open(url)
                        }
                    } label: {
                        HStack {
                            Label("Push Notifications", systemImage: "bell.badge")
                            Spacer()
                            Image(systemName: "arrow.up.forward")
                                .font(.caption)
                                .foregroundStyle(.tertiary)
                        }
                    }
                    .tint(.primary)
                } header: {
                    Text("Notifications").foregroundStyle(.secondary)
                } footer: {
                    Text("Only the pinned primary session sends completion alerts.")
                }

                // MARK: - Support
                Section {
                    Link(destination: URL(string: "mailto:kevin2005ha@gmail.com")!) {
                        Label("Contact Support", systemImage: "envelope")
                    }
                    .tint(.primary)
                    Link(destination: URL(string: "https://github.com/anthropics/claude-code")!) {
                        Label("Claude Code", systemImage: "link")
                    }
                    .tint(.primary)
                } header: {
                    Text("Support").foregroundStyle(.secondary)
                }

                // MARK: - About
                Section {
                    HStack {
                        Text("Version")
                        Spacer()
                        Text(Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "—")
                            .foregroundStyle(.secondary)
                    }
                    Button {
                        UserDefaults.standard.set(false, forKey: "codync_onboardingComplete")
                        dismiss()
                    } label: {
                        Label("Reset Onboarding", systemImage: "arrow.counterclockwise")
                    }
                    .tint(.secondary)
                } header: {
                    Text("About").foregroundStyle(.secondary)
                }
            }
            .tint(.primary)
            .navigationTitle("Settings")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
            .sheet(isPresented: $showPaywall) {
                CodyncPaywallView()
                    .environment(\.theme, theme)
                    .preferredColorScheme(isDarkMode ? .dark : .light)
            }
        }
        .preferredColorScheme(isDarkMode ? .dark : .light)
    }

    @ViewBuilder
    private var subscriptionRow: some View {
        let premium = PremiumManager.shared
        Button {
            if premium.isPro {
                if let url = URL(string: "https://apps.apple.com/account/subscriptions") {
                    UIApplication.shared.open(url)
                }
            } else {
                showPaywall = true
            }
        } label: {
            HStack {
                Label {
                    Text("Codync Pro")
                } icon: {
                    Image(systemName: premium.isPro ? "checkmark.seal.fill" : "seal")
                }
                Spacer()
                if premium.isPro {
                    Image(systemName: "arrow.up.forward")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                } else {
                    Text("$0.99/mo")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    Image(systemName: "chevron.right")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                }
            }
        }
        .tint(.primary)

        if !premium.isPro {
            Button {
                Task {
                    try? await premium.restorePurchases()
                }
            } label: {
                Label("Restore Purchases", systemImage: "arrow.clockwise")
            }
            .tint(.secondary)
        }
    }
}
