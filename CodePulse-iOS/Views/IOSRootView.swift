import SwiftUI
import CodePulseShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager
    @AppStorage("codepulse_darkMode") private var isDarkMode = true

    private var theme: CodePulseTheme { CodePulseTheme(isDark: isDarkMode) }

    var body: some View {
        NavigationStack {
            Group {
                if receiver.sessions.isEmpty {
                    IOSOnboardingView()
                } else {
                    IOSSessionListView(
                        sessions: receiver.sessions,
                        liveActivityManager: liveActivityManager
                    )
                }
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        withAnimation(.easeInOut(duration: 0.2)) { isDarkMode.toggle() }
                    } label: {
                        Image(systemName: isDarkMode ? "moon.fill" : "sun.max.fill")
                            .font(.system(size: 14))
                            .foregroundStyle(theme.secondaryText)
                    }
                }
            }
        }
        .environment(\.theme, theme)
        .preferredColorScheme(isDarkMode ? .dark : .light)
        .onChange(of: receiver.sessions) { _, sessions in
            liveActivityManager.updateSessions(sessions)
        }
    }
}
