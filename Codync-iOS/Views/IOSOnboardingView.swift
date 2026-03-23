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

    private let totalPages = 6

    var body: some View {
        ZStack {
            theme.background.ignoresSafeArea()

            TabView(selection: $currentPage) {
                welcomePage.tag(0)
                macSetupPage.tag(1)
                iCloudPage.tag(2)
                notificationPage.tag(3)
                modePage.tag(4)
                proPage.tag(5)
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .animation(.easeInOut(duration: 0.3), value: currentPage)
        }
        .task {
            await checkICloudStatus()
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

                // Mac mockup with notch
                VStack(spacing: 0) {
                    ZStack(alignment: .top) {
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .fill(Color(white: 0.12))

                        VStack(spacing: 0) {
                            // macOS menu bar with notch
                            HStack(spacing: 0) {
                                // Left menu items
                                HStack(spacing: 6) {
                                    Image(systemName: "apple.logo")
                                        .font(.system(size: 10))
                                    Text("Finder")
                                        .font(.system(size: 9, weight: .semibold))
                                    Text("File")
                                        .font(.system(size: 9))
                                    Text("Edit")
                                        .font(.system(size: 9))
                                }
                                .foregroundStyle(.white.opacity(0.6))
                                .padding(.leading, 10)

                                Spacer()

                                // Notch — centered
                                RoundedRectangle(cornerRadius: 0, style: .continuous)
                                    .fill(.black)
                                    .frame(width: 60, height: 16)
                                    .clipShape(
                                        .rect(
                                            topLeadingRadius: 0,
                                            bottomLeadingRadius: 8,
                                            bottomTrailingRadius: 8,
                                            topTrailingRadius: 0
                                        )
                                    )
                                    .offset(y: -4)

                                Spacer()

                                // Right menu items — Codync icon separated from notch
                                HStack(spacing: 5) {
                                    Image("CodyncIcon")
                                        .resizable()
                                        .aspectRatio(contentMode: .fit)
                                        .frame(width: 11, height: 11)
                                    Text("2")
                                        .font(.system(size: 8, weight: .medium))
                                    Image(systemName: "wifi")
                                        .font(.system(size: 8))
                                    Image(systemName: "battery.75")
                                        .font(.system(size: 8))
                                }
                                .foregroundStyle(.white.opacity(0.6))
                                .padding(.trailing, 10)
                            }
                            .frame(height: 22)
                            .background(.white.opacity(0.06))

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

    // MARK: - Page 6: Pro Upsell

    private var proPage: some View {
        OnboardingPage(
            theme: theme,
            content: {
                Spacer()

                Image(systemName: "bolt.fill")
                    .font(.system(size: 44))
                    .foregroundStyle(theme.primaryText.opacity(0.8))

                Spacer().frame(height: 24)

                Text("Codync Pro")
                    .font(.system(size: 32, weight: .bold))
                    .foregroundStyle(theme.primaryText)

                Spacer().frame(height: 12)

                Text("Real-time sync, even when\nthe app is closed.")
                    .multilineTextAlignment(.center)
                    .font(.system(size: 17))
                    .foregroundStyle(theme.secondaryText)
                    .lineSpacing(4)

                Spacer().frame(height: 8)

                Text("Less than $1/month")
                    .font(.system(size: 15, weight: .medium))
                    .foregroundStyle(theme.primaryText.opacity(0.6))

                Spacer().frame(height: 24)

                VStack(alignment: .leading, spacing: 14) {
                    proFeatureRow(
                        icon: "antenna.radiowaves.left.and.right",
                        text: "Instant updates — under 2 seconds from Mac to iPhone"
                    )
                    proFeatureRow(
                        icon: "livephoto",
                        text: "Live Activity stays updated even when the app is closed or killed"
                    )
                    proFeatureRow(
                        icon: "bell.badge",
                        text: "Session completion alerts delivered in background"
                    )
                    proFeatureRow(
                        icon: "clock.arrow.circlepath",
                        text: "No more stale data — sessions stay fresh indefinitely"
                    )
                }
                .padding(.horizontal, 32)

                Spacer()
            },
            primaryButton: "Start Monitoring",
            primaryAction: {
                liveActivityManager.mode = selectedMode
                Task { await liveActivityManager.savePreference() }
                onboardingComplete = true
            },
            secondaryButton: "Start Free",
            secondaryAction: {
                liveActivityManager.mode = selectedMode
                Task { await liveActivityManager.savePreference() }
                onboardingComplete = true
            }
        )
    }

    private func proFeatureRow(icon: String, text: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: icon)
                .font(.system(size: 15))
                .foregroundStyle(theme.primaryText)
                .frame(width: 28)
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
