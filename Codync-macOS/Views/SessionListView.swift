import SwiftUI
import CodyncShared

struct SessionListView: View {
    @ObservedObject var stateManager: SessionStateManager
    var panelState: CodyncPanelState?
    @AppStorage("codync_darkMode") private var isDarkMode = false
    @State private var selectedSession: SessionState?
    @Environment(\.theme) private var injectedTheme

    private var theme: CodyncTheme {
        injectedTheme.isPanel ? injectedTheme : CodyncTheme(isDark: isDarkMode)
    }

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
        .preferredColorScheme(theme.isDark ? .dark : .light)
    }

    private var sessionList: some View {
        VStack(spacing: 0) {
            if let panelState {
                PanelHeaderView(panelState: panelState)
            }

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
                    .padding(.horizontal, 4)
                }
                .frame(maxHeight: 420)
                .background(theme.cardBackground, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                .padding(.horizontal, 14)
                .padding(.vertical, 6)
            }
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

}
