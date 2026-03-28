import SwiftUI

struct SyncErrorSheet: View {
    let error: CloudKitReceiver.SyncError?
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme

    var body: some View {
        ZStack {
            theme.background.ignoresSafeArea()

            VStack(spacing: 0) {
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
                    Image(systemName: "icloud.slash")
                        .font(.system(size: 40))
                        .foregroundStyle(theme.secondaryText)

                    Text(error?.title ?? "Sync Error")
                        .font(.title2.bold())
                        .foregroundStyle(theme.primaryText)

                    VStack(alignment: .leading, spacing: 12) {
                        if case .quotaExceeded = error {
                            infoRow(
                                icon: "externaldrive.badge.xmark",
                                text: "Your iCloud storage is full. Codync cannot sync session data from your Mac."
                            )
                            infoRow(
                                icon: "clock.arrow.circlepath",
                                text: "Session status shown here may be outdated and won't update until storage is freed."
                            )
                            infoRow(
                                icon: "gearshape",
                                text: "Go to Settings → Apple Account → iCloud → Manage Storage to free up space."
                            )
                        } else if case .networkUnavailable = error {
                            infoRow(
                                icon: "wifi.slash",
                                text: "No internet connection. Session data cannot be synced from your Mac."
                            )
                            infoRow(
                                icon: "clock.arrow.circlepath",
                                text: "Data will refresh automatically when connectivity is restored."
                            )
                        } else {
                            infoRow(
                                icon: "exclamationmark.triangle",
                                text: "An unexpected error occurred while syncing with iCloud."
                            )
                            infoRow(
                                icon: "clock.arrow.circlepath",
                                text: "Codync will retry automatically. If the issue persists, try restarting the app."
                            )
                        }
                    }
                    .padding(.horizontal, 8)
                }
                .padding(.horizontal, 24)

                Spacer()

                if case .quotaExceeded = error {
                    Button {
                        if let url = URL(string: "App-prefs:root=CASTLE&path=STORAGE_AND_BACKUP") {
                            UIApplication.shared.open(url)
                        }
                    } label: {
                        Text("Open iCloud Settings")
                            .font(.headline)
                            .foregroundStyle(theme.isDark ? .black : .white)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 14)
                            .background(theme.primaryText, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                    }
                    .padding(.horizontal, 24)
                    .padding(.bottom, 8)
                }

                Button { dismiss() } label: {
                    Text("Dismiss")
                        .font(.subheadline)
                        .foregroundStyle(theme.tertiaryText)
                }
                .padding(.bottom, 24)
            }
        }
    }

    @ViewBuilder
    private func infoRow(icon: String, text: String) -> some View {
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
