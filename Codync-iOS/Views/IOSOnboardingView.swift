import SwiftUI
import CloudKit
import UserNotifications
import CodyncShared

struct IOSOnboardingView: View {
    @ObservedObject var liveActivityManager: LiveActivityManager
    @Environment(\.theme) private var theme
    @AppStorage("codync_onboardingComplete") private var onboardingComplete = false
    @State private var currentPage = 0
    @State private var iCloudStatus: ICloudStatus = .checking
    @State private var selectedMode: LiveActivityMode = .overall
    @State private var showPaywall = false

    private let totalPages = 8

    var body: some View {
        ZStack {
            theme.background.ignoresSafeArea()

            TabView(selection: $currentPage) {
                welcomePage.tag(0)
                macSetupPage.tag(1)
                iCloudPage.tag(2)
                notificationPage.tag(3)
                modePage.tag(4)
                primarySessionPage.tag(5)
                proPage.tag(6)
                liveActivityPermissionPage.tag(7)
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .animation(.easeInOut(duration: 0.3), value: currentPage)
        }
        .task {
            await checkICloudStatus()
        }
        .sheet(isPresented: $showPaywall) {
            CodyncPaywallView()
                .onDisappear {
                    if PremiumManager.shared.isPro {
                        withAnimation { currentPage = 7 }
                    }
                }
        }
    }

    // MARK: - Page 1: Welcome

    private var welcomePage: some View {
        OnboardingPage(
            theme: theme,
            content: {
                Spacer()

                Image("CodyncIcon")
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(width: 100, height: 100)
                    .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
                    .shadow(color: .white.opacity(0.05), radius: 20)

                Spacer().frame(height: 32)

                Text("Welcome to Codync")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(theme.primaryText)

                Spacer().frame(height: 12)

                Text("Monitor your Claude Code sessions\nin real time from your iPhone")
                    .multilineTextAlignment(.center)
                    .font(.system(size: 17))
                    .foregroundStyle(theme.secondaryText)
                    .lineSpacing(4)

                Spacer()
            },
            primaryButton: "Get Started",
            primaryAction: { withAnimation { currentPage = 1 } }
        )
    }

    // MARK: - Page 2: Install on Mac

    private var macSetupPage: some View {
        OnboardingPage(
            theme: theme,
            content: {
                Spacer()

                // Mac mockup with notch — 1:1 replica of macOS menu bar
                VStack(spacing: 0) {
                    ZStack(alignment: .top) {
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .fill(Color(white: 0.12))

                        VStack(spacing: 0) {
                            // macOS menu bar
                            ZStack(alignment: .top) {
                                // Menu bar background — translucent like real macOS
                                LinearGradient(
                                    colors: [
                                        Color(red: 0.22, green: 0.38, blue: 0.58),
                                        Color(red: 0.28, green: 0.45, blue: 0.65)
                                    ],
                                    startPoint: .leading,
                                    endPoint: .trailing
                                )
                                .frame(height: 24)

                                // Notch with Codync icon inside
                                HStack(spacing: 0) {
                                    Spacer()
                                    HStack(spacing: 0) {
                                        Spacer().frame(width: 50)
                                        Image("CodyncIcon")
                                            .resizable()
                                            .aspectRatio(contentMode: .fit)
                                            .frame(width: 13, height: 13)
                                            .foregroundStyle(.white.opacity(0.9))
                                            .padding(.trailing, 8)
                                    }
                                    .frame(height: 20)
                                    .background(.black)
                                    .clipShape(
                                        .rect(
                                            topLeadingRadius: 0,
                                            bottomLeadingRadius: 10,
                                            bottomTrailingRadius: 10,
                                            topTrailingRadius: 0
                                        )
                                    )
                                    Spacer()
                                }
                                .frame(height: 24, alignment: .top)
                                .offset(x: 15)
                            }
                            .frame(height: 24)
                            .clipShape(
                                .rect(
                                    topLeadingRadius: 12,
                                    bottomLeadingRadius: 0,
                                    bottomTrailingRadius: 0,
                                    topTrailingRadius: 12
                                )
                            )

                            Spacer().frame(height: 8)

                            // Terminal window mockup
                            VStack(alignment: .leading, spacing: 4) {
                                HStack(spacing: 4) {
                                    Circle().fill(.red.opacity(0.6)).frame(width: 7, height: 7)
                                    Circle().fill(.yellow.opacity(0.6)).frame(width: 7, height: 7)
                                    Circle().fill(.green.opacity(0.6)).frame(width: 7, height: 7)
                                    Spacer()
                                }
                                Text("$ claude")
                                    .font(.system(size: 11, design: .monospaced))
                                    .foregroundStyle(.white.opacity(0.6))
                            }
                            .padding(10)
                            .background(
                                RoundedRectangle(cornerRadius: 6, style: .continuous)
                                    .fill(.white.opacity(0.05))
                            )
                            .padding(.horizontal, 10)

                            Spacer()
                        }
                    }
                    .frame(height: 140)
                    .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .stroke(.white.opacity(0.1), lineWidth: 1)
                    )
                }
                .padding(.horizontal, 44)

                Spacer().frame(height: 36)

                Text("Install Codync on Mac")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(theme.primaryText)

                Spacer().frame(height: 12)

                Text("Download Codync on your Mac to\nmonitor Claude Code sessions.")
                    .multilineTextAlignment(.center)
                    .font(.system(size: 17))
                    .foregroundStyle(theme.secondaryText)
                    .lineSpacing(4)

                Spacer()
            },
            primaryButton: "Continue",
            primaryAction: { withAnimation { currentPage = 2 } },
            secondaryButton: "Back",
            secondaryAction: { withAnimation { currentPage = 0 } }
        )
    }

    // MARK: - Page 3: iCloud

    private var iCloudPage: some View {
        OnboardingPage(
            theme: theme,
            content: {
                Spacer()

                // Faux iCloud card
                VStack(spacing: 0) {
                    RoundedRectangle(cornerRadius: 16, style: .continuous)
                        .fill(Color(white: 0.95))
                        .frame(height: 200)
                        .overlay(
                            VStack(spacing: 12) {
                                Image(systemName: "icloud.fill")
                                    .font(.system(size: 40))
                                    .foregroundStyle(.gray.opacity(0.4))
                                HStack(spacing: 8) {
                                    Circle().fill(.gray.opacity(0.15)).frame(width: 36, height: 36)
                                    VStack(alignment: .leading, spacing: 4) {
                                        RoundedRectangle(cornerRadius: 3).fill(.gray.opacity(0.15)).frame(width: 120, height: 10)
                                        RoundedRectangle(cornerRadius: 3).fill(.gray.opacity(0.1)).frame(width: 80, height: 8)
                                    }
                                    Spacer()
                                    RoundedRectangle(cornerRadius: 6).fill(.gray.opacity(0.12)).frame(width: 50, height: 24)
                                }
                                .padding(.horizontal, 16)
                            }
                        )
                        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                }
                .padding(.horizontal, 40)

                Spacer().frame(height: 40)

                Text("iCloud Sync")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(theme.primaryText)

                Spacer().frame(height: 12)

                Text("Session status, tasks, and costs sync\nbetween your Mac and iPhone via iCloud.")
                    .multilineTextAlignment(.center)
                    .font(.system(size: 17))
                    .foregroundStyle(theme.secondaryText)
                    .lineSpacing(4)

                Spacer().frame(height: 14)

                HStack(spacing: 6) {
                    Image(systemName: "person.crop.circle")
                        .font(.system(size: 13))
                        .foregroundStyle(theme.tertiaryText)
                    Text("Make sure both devices use the same Apple ID")
                        .font(.system(size: 13))
                        .foregroundStyle(theme.tertiaryText)
                }

                Spacer().frame(height: 14)

                // Status badge
                HStack(spacing: 6) {
                    if iCloudStatus == .checking {
                        ProgressView().scaleEffect(0.7)
                    } else if iCloudStatus == .available {
                        Image(systemName: "checkmark.circle.fill")
                            .foregroundStyle(.white)
                    } else {
                        Image(systemName: "exclamationmark.circle.fill")
                            .foregroundStyle(.white.opacity(0.7))
                    }
                    Text(iCloudSubtitle)
                        .font(.system(size: 14))
                        .foregroundStyle(theme.secondaryText)
                }

                // Action button for iCloud issues
                if iCloudStatus == .noAccount || iCloudStatus == .restricted {
                    Button {
                        if let url = URL(string: UIApplication.openSettingsURLString) {
                            UIApplication.shared.open(url)
                        }
                    } label: {
                        Text("Open Settings")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(theme.primaryText)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)
                            .background(
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .fill(theme.primaryText.opacity(0.1))
                            )
                    }
                    .padding(.top, 8)
                } else if iCloudStatus == .quotaExceeded {
                    Button {
                        // Open iCloud storage settings
                        if let url = URL(string: "App-Prefs:root=CASTLE&path=STORAGE_AND_BACKUP") {
                            UIApplication.shared.open(url)
                        }
                    } label: {
                        Text("Manage iCloud Storage")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(theme.primaryText)
                            .padding(.horizontal, 16)
                            .padding(.vertical, 8)
                            .background(
                                RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .fill(theme.primaryText.opacity(0.1))
                            )
                    }
                    .padding(.top, 8)
                }

                Spacer()
            },
            primaryButton: iCloudStatus.isError ? "Retry" : "Continue",
            primaryAction: {
                if iCloudStatus.isError {
                    iCloudStatus = .checking
                    Task { await checkICloudStatus() }
                } else {
                    withAnimation { currentPage = 3 }
                }
            },
            secondaryButton: iCloudStatus.isError ? "Skip" : "Back",
            secondaryAction: {
                if iCloudStatus.isError {
                    withAnimation { currentPage = 3 }
                } else {
                    withAnimation { currentPage = 1 }
                }
            }
        )
    }

    // MARK: - Page 3: Notifications

    private var notificationPage: some View {
        NotificationOnboardingPage(
            theme: theme,
            onContinue: { withAnimation { currentPage = 4 } },
            onSkip: { withAnimation { currentPage = 4 } }
        )
    }

    // MARK: - Page 4: Mode Selection

    private var modePage: some View {
        OnboardingPage(
            theme: theme,
            content: {
                Spacer()

                Text("Live Activity Format")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(theme.primaryText)

                Spacer().frame(height: 12)

                Text("Choose how sessions appear\non your Dynamic Island and Lock Screen.")
                    .multilineTextAlignment(.center)
                    .font(.system(size: 17))
                    .foregroundStyle(theme.secondaryText)
                    .lineSpacing(4)

                Spacer().frame(height: 10)

                HStack(spacing: 5) {
                    Image(systemName: "clock")
                        .font(.system(size: 12))
                    Text("Updates may have a slight delay from Mac")
                        .font(.system(size: 13))
                }
                .foregroundStyle(theme.tertiaryText)

                Spacer().frame(height: 20)

                ModePillToggle(mode: $selectedMode)
                    .padding(.horizontal, 24)

                Spacer().frame(height: 16)

                LiveActivityPreview(mode: selectedMode)
                    .padding(.horizontal, 24)

                Spacer()
            },
            primaryButton: "Continue",
            primaryAction: { withAnimation { currentPage = 5 } },
            secondaryButton: "Back",
            secondaryAction: { withAnimation { currentPage = 3 } }
        )
    }

    // MARK: - Page 6: Primary Session

    @State private var demoPrimary = 0

    private var primarySessionPage: some View {
        OnboardingPage(
            theme: theme,
            content: {
                Spacer()

                // Interactive demo list
                let sessions = [
                    ("Codync", "Opus"),
                    ("MyApp", "Sonnet"),
                    ("Backend", "Haiku"),
                ]
                VStack(spacing: 2) {
                    ForEach(Array(sessions.enumerated()), id: \.offset) { index, session in
                        let isPrimary = index == demoPrimary
                        HStack(spacing: 8) {
                            Circle()
                                .fill(theme.primaryText.opacity(isPrimary ? 1 : 0.35))
                                .frame(width: 7, height: 7)
                            Text(session.0)
                                .font(.system(size: 14, weight: isPrimary ? .semibold : .regular))
                                .foregroundStyle(theme.primaryText)
                            Text(session.1)
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(theme.secondaryText)
                                .padding(.horizontal, 6)
                                .padding(.vertical, 2)
                                .background(theme.primaryText.opacity(0.08), in: Capsule())
                            Spacer()
                            Button {
                                withAnimation(.spring(duration: 0.3)) { demoPrimary = index }
                            } label: {
                                Circle()
                                    .fill(isPrimary ? theme.primaryText : theme.tertiaryText.opacity(0.3))
                                    .frame(width: isPrimary ? 8 : 6, height: isPrimary ? 8 : 6)
                                    .frame(width: 28, height: 28)
                                    .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 8)
                        .background(
                            isPrimary
                                ? RoundedRectangle(cornerRadius: 8, style: .continuous)
                                    .fill(theme.primaryText.opacity(0.06))
                                : nil
                        )
                    }
                }
                .padding(10)
                .background(
                    RoundedRectangle(cornerRadius: 16, style: .continuous)
                        .fill(theme.cardBackground)
                        .overlay(
                            RoundedRectangle(cornerRadius: 16, style: .continuous)
                                .stroke(theme.border, lineWidth: 1)
                        )
                )
                .padding(.horizontal, 40)

                Spacer().frame(height: 36)

                Text("Primary Session")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(theme.primaryText)

                Spacer().frame(height: 12)

                Text("Tap the circle on the right to try it.\nShown first on Dynamic Island and Lock Screen.")
                    .multilineTextAlignment(.center)
                    .font(.system(size: 16))
                    .foregroundStyle(theme.secondaryText)
                    .lineSpacing(4)

                Spacer()
            },
            primaryButton: "Continue",
            primaryAction: { withAnimation { currentPage = 6 } },
            secondaryButton: "Back",
            secondaryAction: { withAnimation { currentPage = 4 } }
        )
    }

    // MARK: - Page 7: Pro Upsell

    private var proPage: some View {
        OnboardingPage(
            theme: theme,
            content: {
                Spacer()

                Image(systemName: "crown.fill")
                    .font(.system(size: 44))
                    .foregroundStyle(theme.primaryText)

                Spacer().frame(height: 20)

                HStack(spacing: 8) {
                    Text("Codync Pro")
                        .font(.system(size: 32, weight: .bold))
                    Text("$0.99/mo")
                        .font(.system(size: 17, weight: .medium))
                        .foregroundStyle(theme.secondaryText)
                }
                .foregroundStyle(theme.primaryText)

                Spacer().frame(height: 14)

                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .font(.system(size: 15))
                    Text("Free: sync stops after the app is closed for a while")
                        .font(.system(size: 15, weight: .semibold))
                }
                .foregroundStyle(theme.primaryText)
                .padding(.horizontal, 28)

                Spacer().frame(height: 20)

                VStack(alignment: .leading, spacing: 14) {
                    proFeatureRow("Instant push updates to Live Activity")
                    proFeatureRow("Works even when app is closed")
                    proFeatureRow("Background completion alerts")
                }
                .padding(.horizontal, 40)

                Spacer()
            },
            primaryButton: PremiumManager.shared.isPro ? "Continue" : "Subscribe & Start",
            primaryAction: {
                if PremiumManager.shared.isPro {
                    withAnimation { currentPage = 7 }
                } else {
                    showPaywall = true
                }
            },
            secondaryButton: PremiumManager.shared.isPro ? "Back" : "Continue Free",
            secondaryAction: {
                withAnimation { currentPage = 7 }
            }
        )
    }

    // MARK: - Page 8: Live Activity Permission

    private var liveActivityPermissionPage: some View {
        OnboardingPage(
            theme: theme,
            content: {
                Spacer()

                // Faux Live Activity permission dialog — mimics real iOS alert
                VStack(spacing: 0) {
                    // Session list (dark card)
                    VStack(spacing: 2) {
                        fauxLARow(name: "my-project", model: "Opus", task: "Reading file...")
                        fauxLARow(name: "backend", model: "Sonnet", task: nil)
                        fauxLARow(name: "web-app", model: "Haiku", task: "Editing code")
                    }
                    .padding(12)
                    .background(
                        RoundedRectangle(cornerRadius: 16, style: .continuous)
                            .fill(Color(white: 0.13))
                    )

                    Spacer().frame(height: 8)

                    // Permission dialog — frosted glass style
                    VStack(spacing: 0) {
                        Text("Allow Live Activities\nfrom \u{201C}Codync\u{201D}?")
                            .font(.system(size: 16, weight: .medium))
                            .foregroundStyle(.white.opacity(0.9))
                            .multilineTextAlignment(.center)
                            .lineSpacing(3)
                            .padding(.top, 18)
                            .padding(.bottom, 16)
                            .padding(.horizontal, 16)

                        Divider().overlay(.white.opacity(0.12))

                        HStack(spacing: 0) {
                            Button {} label: {
                                Text("Don't Allow")
                                    .font(.system(size: 16))
                                    .foregroundStyle(.white.opacity(0.55))
                                    .frame(maxWidth: .infinity)
                                    .padding(.vertical, 14)
                            }

                            Divider().overlay(.white.opacity(0.12)).frame(height: 44)

                            Button {} label: {
                                Text("Allow")
                                    .font(.system(size: 16, weight: .semibold))
                                    .foregroundStyle(.white.opacity(0.9))
                                    .frame(maxWidth: .infinity)
                                    .padding(.vertical, 14)
                            }
                        }
                    }
                    .background(
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .fill(.ultraThinMaterial)
                            .environment(\.colorScheme, .dark)
                    )
                    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                }
                .padding(.horizontal, 36)

                Spacer().frame(height: 32)

                Text("Allow Live Activities")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(theme.primaryText)

                Spacer().frame(height: 12)

                Text("This is not a real prompt — just a preview.\nWhen iOS asks you later, tap \u{201C}Allow\u{201D}\nfor real-time Dynamic Island updates.")
                    .multilineTextAlignment(.center)
                    .font(.system(size: 16))
                    .foregroundStyle(theme.secondaryText)
                    .lineSpacing(4)

                Spacer()
            },
            primaryButton: "Start Monitoring",
            primaryAction: {
                liveActivityManager.mode = selectedMode
                Task { await liveActivityManager.savePreference() }
                onboardingComplete = true
            },
            secondaryButton: "Back",
            secondaryAction: { withAnimation { currentPage = 6 } }
        )
    }

    private func fauxLARow(name: String, model: String, task: String?) -> some View {
        HStack(spacing: 8) {
            Circle()
                .fill(.white.opacity(task != nil ? 0.8 : 0.35))
                .frame(width: 7, height: 7)
            Text(name)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.white)
            Text(model)
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.white.opacity(0.4))
                .padding(.horizontal, 5)
                .padding(.vertical, 2)
                .background(.white.opacity(0.08), in: Capsule())
            Spacer()
            if let task {
                Text(task)
                    .font(.system(size: 11))
                    .foregroundStyle(.white.opacity(0.35))
                    .lineLimit(1)
            }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
    }

    private func proFeatureRow(_ text: String) -> some View {
        HStack(spacing: 10) {
            Image(systemName: "checkmark")
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(theme.primaryText)
            Text(text)
                .font(.system(size: 15))
                .foregroundStyle(theme.secondaryText)
        }
    }

    // MARK: - Helpers

    private var iCloudSubtitle: String {
        switch iCloudStatus {
        case .checking: return "Checking iCloud status…"
        case .available: return "iCloud is ready"
        case .noAccount: return "Sign in to iCloud in Settings"
        case .restricted: return "iCloud is restricted on this device"
        case .quotaExceeded: return "iCloud storage is full"
        case .error(let msg): return msg
        }
    }

    private func checkICloudStatus() async {
        do {
            let status = try await CKContainer.default().accountStatus()
            switch status {
            case .available:
                // Account OK — verify CloudKit zone works (catches quota issues)
                do {
                    try await CloudKitManager.shared.ensureZoneExists()
                    await MainActor.run { iCloudStatus = .available }
                } catch let error as CKError where error.code == .quotaExceeded {
                    await MainActor.run { iCloudStatus = .quotaExceeded }
                } catch {
                    // Zone might already exist, still OK
                    await MainActor.run { iCloudStatus = .available }
                }
            case .noAccount:
                await MainActor.run { iCloudStatus = .noAccount }
            case .restricted:
                await MainActor.run { iCloudStatus = .restricted }
            default:
                await MainActor.run { iCloudStatus = .error("iCloud unavailable") }
            }
        } catch {
            let nsError = error as NSError
            await MainActor.run {
                if nsError.code == CKError.quotaExceeded.rawValue {
                    iCloudStatus = .quotaExceeded
                } else {
                    iCloudStatus = .error("iCloud: \(error.localizedDescription)")
                }
            }
        }
    }
}

// MARK: - Reusable Page Layout

private struct OnboardingPage<Content: View>: View {
    let theme: CodyncTheme
    @ViewBuilder let content: () -> Content
    let primaryButton: String
    let primaryAction: () -> Void
    var secondaryButton: String? = nil
    var secondaryAction: (() -> Void)? = nil

    var body: some View {
        VStack(spacing: 0) {
            content()

            // Buttons
            VStack(spacing: 12) {
                Button(action: primaryAction) {
                    Text(primaryButton)
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(theme.isDark ? .black : .white)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 16)
                        .background(
                            RoundedRectangle(cornerRadius: 14, style: .continuous)
                                .fill(theme.primaryText)
                        )
                }

                if let secondaryButton, let secondaryAction {
                    Button(action: secondaryAction) {
                        Text(secondaryButton)
                            .font(.system(size: 15, weight: .medium))
                            .foregroundStyle(theme.secondaryText)
                    }
                }
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 50)
        }
    }
}

// MARK: - Notification Page (animated)

private struct NotificationOnboardingPage: View {
    let theme: CodyncTheme
    let onContinue: () -> Void
    let onSkip: () -> Void

    @State private var notificationEnabled = false
    @State private var showBanner = false
    @State private var appeared = false

    var body: some View {
        VStack(spacing: 0) {
            Spacer().frame(height: 60)

            // Faux iPhone screen
            VStack(spacing: 0) {
                // iPhone frame with notch
                ZStack(alignment: .top) {
                    RoundedRectangle(cornerRadius: 28, style: .continuous)
                        .fill(Color(white: 0.94))

                    VStack(spacing: 0) {
                        // Top spacer + Notch
                        Spacer().frame(height: 8)

                        RoundedRectangle(cornerRadius: 20, style: .continuous)
                            .fill(.black)
                            .frame(width: 100, height: 28)

                        // Status bar
                        HStack {
                            Text("9:41")
                                .font(.system(size: 12, weight: .semibold))
                            Spacer()
                            HStack(spacing: 3) {
                                Image(systemName: "cellularbars")
                                Image(systemName: "wifi")
                                Image(systemName: "battery.100")
                            }
                            .font(.system(size: 10))
                        }
                        .foregroundStyle(.black.opacity(0.35))
                        .padding(.horizontal, 20)
                        .padding(.top, 6)

                        Spacer().frame(height: 8)

                        // Notification banner — slides down from top
                        HStack(spacing: 10) {
                            Image("CodyncIcon")
                                .resizable()
                                .aspectRatio(contentMode: .fit)
                                .frame(width: 34, height: 34)
                                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))

                            VStack(alignment: .leading, spacing: 2) {
                                HStack {
                                    Text("Session Complete")
                                        .font(.system(size: 13, weight: .semibold))
                                        .foregroundStyle(.black)
                                    Spacer()
                                    Text("now")
                                        .font(.system(size: 11))
                                        .foregroundStyle(.black.opacity(0.35))
                                }
                                Text("my-project finished · $0.42")
                                    .font(.system(size: 12))
                                    .foregroundStyle(.black.opacity(0.45))
                            }
                        }
                        .padding(10)
                        .background(
                            RoundedRectangle(cornerRadius: 16, style: .continuous)
                                .fill(.white)
                                .shadow(color: .black.opacity(0.06), radius: 10, y: 3)
                        )
                        .padding(.horizontal, 10)
                        .offset(y: showBanner ? 0 : -150)

                        Spacer()

                        // Faux app grid
                        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 4), spacing: 8) {
                            ForEach(0..<8, id: \.self) { _ in
                                RoundedRectangle(cornerRadius: 12, style: .continuous)
                                    .fill(.gray.opacity(0.08))
                                    .aspectRatio(1, contentMode: .fit)
                            }
                        }
                        .padding(.horizontal, 14)
                        .padding(.bottom, 12)
                    }
                }
                .frame(height: 300)
                .clipShape(RoundedRectangle(cornerRadius: 28, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 28, style: .continuous)
                        .stroke(.white.opacity(0.1), lineWidth: 1)
                )
            }
            .padding(.horizontal, 36)

            Spacer().frame(height: 40)

            Text("Stay Updated with\nPush Notifications")
                .multilineTextAlignment(.center)
                .font(.system(size: 32, weight: .bold))
                .foregroundStyle(theme.primaryText)

            Spacer().frame(height: 12)

            Text("Get notified when your Claude Code\nsessions complete, even in background.")
                .multilineTextAlignment(.center)
                .font(.system(size: 17))
                .foregroundStyle(theme.secondaryText)
                .lineSpacing(4)

            Spacer()

            // Buttons
            VStack(spacing: 12) {
                Button {
                    if notificationEnabled {
                        onContinue()
                    } else {
                        Task {
                            let center = UNUserNotificationCenter.current()
                            let granted = (try? await center.requestAuthorization(options: [.alert, .sound, .badge])) ?? false
                            if granted {
                                // Send a test notification
                                let content = UNMutableNotificationContent()
                                content.title = "You're all set!"
                                content.body = "Codync will notify you when sessions complete."
                                content.sound = .default
                                let request = UNNotificationRequest(identifier: "onboarding-test", content: content, trigger: UNTimeIntervalNotificationTrigger(timeInterval: 1, repeats: false))
                                try? await center.add(request)
                            }
                            await MainActor.run {
                                if granted {
                                    withAnimation(.spring(duration: 0.4)) {
                                        notificationEnabled = true
                                    }
                                }
                            }
                        }
                    }
                } label: {
                    Text(notificationEnabled ? "Continue" : "Enable Push Notifications")
                        .font(.system(size: 17, weight: .semibold))
                        .foregroundStyle(theme.isDark ? .black : .white)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 16)
                        .background(
                            RoundedRectangle(cornerRadius: 14, style: .continuous)
                                .fill(theme.primaryText)
                        )
                }

                Button(action: onSkip) {
                    Text("Ask Me Later")
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(theme.secondaryText)
                }
            }
            .padding(.horizontal, 24)
            .padding(.bottom, 50)
        }
        .onAppear {
            guard !appeared else { return }
            appeared = true
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
                withAnimation(.spring(duration: 0.7, bounce: 0.25)) {
                    showBanner = true
                }
            }
        }
        .onDisappear {
            showBanner = false
            appeared = false
        }
    }
}

// MARK: - ICloud Status

private enum ICloudStatus: Equatable {
    case checking, available, noAccount, restricted, quotaExceeded
    case error(String)

    var isError: Bool {
        switch self {
        case .noAccount, .restricted, .quotaExceeded, .error: return true
        default: return false
        }
    }
}
