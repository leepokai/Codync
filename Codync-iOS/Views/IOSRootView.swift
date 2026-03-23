import SwiftUI
import CodyncShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager
    @ObservedObject var primarySessionManager: PrimarySessionManager
    @AppStorage("codync_onboardingComplete") private var onboardingComplete = false
    @AppStorage("codync_darkMode") private var storedDarkMode = true
    @State private var isDarkMode = true
    @State private var displayedSessions: [SessionState] = []
    @State private var reorderTimer: Timer?
    @State private var showSplash = true

    var body: some View {
        Group {
            if showSplash {
                splashView
            } else if !onboardingComplete {
                NavigationStack {
                    IOSOnboardingView(liveActivityManager: liveActivityManager)
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
        .onAppear { isDarkMode = storedDarkMode }
        .onChange(of: storedDarkMode) { _, newValue in
            withAnimation(.easeInOut(duration: 0.4)) {
                isDarkMode = newValue
            }
        }
        .onChange(of: receiver.sessions) { _, sessions in
            liveActivityManager.userPrimarySessionId = primarySessionManager.primarySessionId
            liveActivityManager.updateSessions(sessions)
            primarySessionManager.autoSelect(from: sessions)
        }
        .onChange(of: primarySessionManager.primarySessionId) { _, newId in
            liveActivityManager.userPrimarySessionId = newId
            liveActivityManager.updateSessions(receiver.sessions)
            withAnimation(.spring(duration: 0.5, bounce: 0.1)) {
                displayedSessions = sortSessions(receiver.sessions)
            }
        }
        .task {
            liveActivityManager.userPrimarySessionId = primarySessionManager.primarySessionId
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
        liveActivityManager.sortedSessions(sessions)
    }
}
