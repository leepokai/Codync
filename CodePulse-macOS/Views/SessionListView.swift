import SwiftUI
import CodePulseShared

struct SessionListView: View {
    @ObservedObject var stateManager: SessionStateManager
    @AppStorage("codepulse_darkMode") private var isDarkMode = false
    @State private var selectedSession: SessionState?

    private var theme: CodePulseTheme { CodePulseTheme(isDark: isDarkMode) }

    var body: some View {
        VStack(spacing: 0) {
            if let selected = selectedSession {
                let liveSession = stateManager.sessions.first { $0.sessionId == selected.sessionId } ?? selected
                SessionDetailView(session: liveSession) {
                    withAnimation(.easeInOut(duration: 0.2)) { selectedSession = nil }
                }
            } else {
                sessionList
            }
        }
        .frame(width: 320)
        .fixedSize(horizontal: true, vertical: true)
        .background(theme.background)
        .environment(\.theme, theme)
        .preferredColorScheme(isDarkMode ? .dark : .light)
    }

    private var sessionList: some View {
        VStack(spacing: 0) {
            if stateManager.sessions.isEmpty {
                emptyState
            } else {
                ScrollView(.vertical, showsIndicators: false) {
                    VStack(spacing: 2) {
                        ForEach(stateManager.sessions) { session in
                            SessionRowView(session: session) {
                                withAnimation(.easeInOut(duration: 0.2)) { selectedSession = session }
                            }
                        }
                    }
                    .padding(.vertical, 6)
                    .padding(.horizontal, 2)
                }
                .frame(maxHeight: 420)
            }

            Divider().padding(.horizontal, 8)

            footer
        }
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Image(systemName: "terminal")
                .font(.system(size: 24, weight: .light))
                .foregroundStyle(theme.secondaryText)
            Text("No active sessions")
                .font(.system(size: 13))
                .foregroundStyle(theme.secondaryText)
        }
        .frame(height: 120)
        .frame(maxWidth: .infinity)
    }

    private var footer: some View {
        HStack(alignment: .center, spacing: 8) {
            Text("CodePulse")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(theme.secondaryText.opacity(0.6))

            Spacer()

            Button(action: { isDarkMode.toggle() }) {
                Image(systemName: isDarkMode ? "sun.max" : "moon")
                    .font(.system(size: 11))
                    .foregroundStyle(theme.secondaryText)
            }
            .buttonStyle(.plain)

            Button(action: { NSApp.terminate(nil) }) {
                Text("Quit")
                    .font(.system(size: 11))
                    .foregroundStyle(theme.secondaryText)
            }
            .buttonStyle(.plain)
            .onHover { inside in
                if inside { NSCursor.pointingHand.push() }
                else { NSCursor.pop() }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }
}
