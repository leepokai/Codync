import SwiftUI
import CodyncShared

struct SessionListView: View {
    @ObservedObject var stateManager: SessionStateManager
    var panelState: CodyncPanelState?
    @AppStorage("codync_darkMode") private var storedDarkMode = true
    @State private var isDarkMode = true
    @State private var selectedSession: SessionState?
    @State private var displayedSessions: [SessionState] = []
    @State private var reorderTimer: Timer?
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
        .onAppear { isDarkMode = storedDarkMode }
        .onChange(of: storedDarkMode) { _, newValue in
            withAnimation(.easeInOut(duration: 0.6)) { isDarkMode = newValue }
        }
        .task {
            displayedSessions = stateManager.sessions
            reorderTimer?.invalidate()
            reorderTimer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { _ in
                Task { @MainActor in
                    let sorted = stateManager.sessions
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

    private var sessionList: some View {
        VStack(spacing: 0) {
            if let panelState {
                PanelHeaderView(panelState: panelState)
            }

            if displayedSessions.isEmpty && stateManager.sessions.isEmpty {
                emptyState
            } else {
                ScrollView(.vertical, showsIndicators: false) {
                    VStack(spacing: 2) {
                        ForEach(displayedSessions) { session in
                            SessionRowView(session: session) {
                                withAnimation(.easeInOut(duration: 0.2)) { selectedSession = session }
                            }
                        }
                    }
                    .padding(.vertical, 6)
                    .padding(.horizontal, 4)
                }
                .environment(\.theme, CodyncTheme(isDark: isDarkMode, isPanel: false))
                .frame(maxHeight: 420)
                .background(
                    (isDarkMode ? theme.cardBackground : Color.white.opacity(0.85)),
                    in: RoundedRectangle(cornerRadius: 12, style: .continuous)
                )
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
