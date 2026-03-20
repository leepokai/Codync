import SwiftUI
import CodyncShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager
    @AppStorage("codync_onboardingComplete") private var onboardingComplete = false
    @AppStorage("codync_darkMode") private var isDarkMode = true
    @State private var displayedSessions: [SessionState] = []
    @State private var reorderTimer: Timer?

    var body: some View {
        NavigationStack {
            Group {
                if !onboardingComplete {
                    IOSOnboardingView()
                } else {
                    IOSSessionListView(
                        sessions: displayedSessions,
                        liveActivityManager: liveActivityManager
                    )
                }
            }
        }
        .environment(\.theme, CodyncTheme(isDark: isDarkMode))
        .preferredColorScheme(isDarkMode ? .dark : .light)
        .onChange(of: receiver.sessions) { _, sessions in
            if !sessions.isEmpty {
                onboardingComplete = true
            }
            liveActivityManager.updateSessions(sessions)
        }
        .task {
            // Initialize with current sessions
            displayedSessions = sortSessions(receiver.sessions)
            // Start 5-second reorder timer
            reorderTimer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { _ in
                Task { @MainActor in
                    let sorted = sortSessions(receiver.sessions)
                    withAnimation(.spring(duration: 2.0, bounce: 0.1)) {
                        displayedSessions = sorted
                    }
                }
            }
        }
        .onDisappear {
            reorderTimer?.invalidate()
            reorderTimer = nil
        }
    }

    private func sortSessions(_ sessions: [SessionState]) -> [SessionState] {
        sessions.sorted { a, b in
            let aWeight = a.status == .working ? 0 : a.status == .needsInput ? 1 : 2
            let bWeight = b.status == .working ? 0 : b.status == .needsInput ? 1 : 2
            if aWeight != bWeight { return aWeight < bWeight }
            return a.updatedAt > b.updatedAt
        }
    }
}
