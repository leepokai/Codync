import SwiftUI
import CodyncShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager
    @ObservedObject var primarySessionManager: PrimarySessionManager
    @AppStorage("codync_onboardingComplete") private var onboardingComplete = false
    @AppStorage("codync_darkMode") private var isDarkMode = true
    @State private var displayedSessions: [SessionState] = []
    @State private var reorderTimer: Timer?
    @State private var showSplash = true

    var body: some View {
        Group {
            if showSplash {
                splashView
            } else if !onboardingComplete {
                NavigationStack {
                    IOSOnboardingView()
                }
            } else {
                TabRootView(
                    sessions: displayedSessions,
                    liveActivityManager: liveActivityManager,
                    primarySessionManager: primarySessionManager
                )
            }
        }
        .environment(\.theme, CodyncTheme(isDark: isDarkMode))
        .preferredColorScheme(isDarkMode ? .dark : .light)
        .onChange(of: receiver.sessions) { _, sessions in
            if !sessions.isEmpty {
                onboardingComplete = true
            }
            liveActivityManager.updateSessions(sessions)
            primarySessionManager.autoSelect(from: sessions)
        }
        .task {
            displayedSessions = sortSessions(receiver.sessions)
            reorderTimer?.invalidate()
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

    private var splashView: some View {
        ZStack {
            Color(red: 0.06, green: 0.06, blue: 0.07)
                .ignoresSafeArea()
            Image("CodyncIcon")
                .resizable()
                .aspectRatio(contentMode: .fit)
                .frame(width: 80, height: 80)
                .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .onAppear {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) {
                withAnimation(.easeOut(duration: 0.3)) {
                    showSplash = false
                }
            }
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
