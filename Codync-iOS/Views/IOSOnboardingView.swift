import SwiftUI
import CloudKit
import CodyncShared

struct IOSOnboardingView: View {
    @ObservedObject var liveActivityManager: LiveActivityManager
    @Environment(\.theme) private var theme
    @AppStorage("codync_onboardingComplete") private var onboardingComplete = false
    @State private var iCloudStatus: ICloudStatus = .checking
    @State private var selectedMode: LiveActivityMode = .overall

    var body: some View {
        VStack(spacing: 0) {
            Spacer()

            Image(systemName: "waveform.path")
                .font(.system(size: 56, weight: .light))
                .foregroundStyle(theme.primaryText.opacity(0.4))
                .padding(.bottom, 20)

            Text("Codync")
                .font(.system(size: 28, weight: .semibold, design: .rounded))
                .foregroundStyle(theme.primaryText)
                .padding(.bottom, 6)

            Text("Monitor your Claude Code sessions\nin real time from your iPhone")
                .multilineTextAlignment(.center)
                .font(.system(size: 15))
                .foregroundStyle(theme.secondaryText)
                .padding(.bottom, 28)

            VStack(alignment: .leading, spacing: 16) {
                stepRow(
                    icon: "icloud",
                    title: "iCloud Sync",
                    subtitle: iCloudSubtitle,
                    isOK: iCloudStatus == .available,
                    isError: iCloudStatus.isError,
                    isLoading: iCloudStatus == .checking
                )
                stepRow(
                    icon: "desktopcomputer",
                    title: "Install on Mac",
                    subtitle: "Install Codync on your Mac to sync sessions",
                    isOK: false,
                    isError: false,
                    isLoading: false
                )
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 20)
            .background(
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(theme.cardBackground)
                    .overlay(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .stroke(theme.border, lineWidth: 1)
                    )
            )
            .padding(.horizontal, 24)

            // Mode selection
            VStack(spacing: 12) {
                Text("Live Activity Format")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(theme.secondaryText)
                    .frame(maxWidth: .infinity, alignment: .leading)

                ModePillToggle(mode: $selectedMode)
                LiveActivityPreview(mode: selectedMode)
            }
            .padding(.horizontal, 24)
            .padding(.top, 20)

            Spacer()

            Button {
                liveActivityManager.mode = selectedMode
                Task { await liveActivityManager.savePreference() }
                onboardingComplete = true
            } label: {
                Text("Get Started")
                    .font(.system(size: 17, weight: .semibold))
                    .foregroundStyle(theme.isDark ? .black : .white)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 14)
                    .background(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .fill(iCloudStatus.isError
                                  ? theme.primaryText.opacity(0.2)
                                  : theme.primaryText)
                    )
            }
            .disabled(iCloudStatus.isError)
            .padding(.horizontal, 24)
            .padding(.bottom, 32)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(theme.background.ignoresSafeArea())
        .task {
            await checkICloudStatus()
        }
    }

    private var iCloudSubtitle: String {
        switch iCloudStatus {
        case .checking: return "Checking iCloud status…"
        case .available: return "iCloud is ready"
        case .noAccount: return "Sign in to iCloud in Settings"
        case .restricted: return "iCloud is restricted on this device"
        case .quotaExceeded: return "iCloud 储存空间已满 — 前往 iCloud 相簿删除一张照片以释放空间"
        case .error(let msg): return msg
        }
    }

    private func stepRow(
        icon: String, title: String, subtitle: String,
        isOK: Bool, isError: Bool, isLoading: Bool
    ) -> some View {
        let iconColor = isOK ? theme.primaryText : isError ? theme.primaryText : theme.tertiaryText

        return HStack(spacing: 12) {
            Image(systemName: icon)
                .font(.system(size: 16))
                .foregroundStyle(iconColor)
                .frame(width: 32, height: 32)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(iconColor.opacity(0.1))
                )

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(title)
                        .font(.system(size: 15, weight: .medium))
                        .foregroundStyle(theme.primaryText)
                    if isLoading {
                        ProgressView()
                            .scaleEffect(0.6)
                            .frame(width: 14, height: 14)
                    } else if isOK {
                        Image(systemName: "checkmark.circle.fill")
                            .font(.system(size: 13))
                            .foregroundStyle(theme.primaryText.opacity(0.7))
                    } else if isError {
                        Image(systemName: "exclamationmark.circle.fill")
                            .font(.system(size: 13))
                            .foregroundStyle(theme.primaryText.opacity(0.7))
                    } else {
                        Circle()
                            .fill(theme.tertiaryText)
                            .frame(width: 8, height: 8)
                    }
                }
                Text(subtitle)
                    .font(.system(size: 13))
                    .foregroundStyle(theme.secondaryText)
                    .lineLimit(2)
            }
        }
    }

    private func checkICloudStatus() async {
        do {
            let status = try await CKContainer.default().accountStatus()
            await MainActor.run {
                switch status {
                case .available: iCloudStatus = .available
                case .noAccount: iCloudStatus = .noAccount
                case .restricted: iCloudStatus = .restricted
                default: iCloudStatus = .error("iCloud unavailable")
                }
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

private enum ICloudStatus: Equatable {
    case checking
    case available
    case noAccount
    case restricted
    case quotaExceeded
    case error(String)

    var isError: Bool {
        switch self {
        case .noAccount, .restricted, .quotaExceeded, .error: return true
        default: return false
        }
    }
}
