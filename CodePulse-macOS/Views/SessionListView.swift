import SwiftUI
import CodePulseShared

struct SessionListView: View {
    @ObservedObject var stateManager: SessionStateManager
    @State private var selectedSession: SessionState?

    var body: some View {
        VStack(spacing: 0) {
            if let selected = selectedSession {
                // Find the live version of this session
                let liveSession = stateManager.sessions.first { $0.sessionId == selected.sessionId } ?? selected
                SessionDetailView(session: liveSession) {
                    withAnimation(.easeInOut(duration: 0.2)) { selectedSession = nil }
                }
            } else {
                sessionList
            }
        }
        .frame(width: 320, height: 380)
    }

    private var sessionList: some View {
        VStack(spacing: 0) {
            if stateManager.sessions.isEmpty {
                emptyState
            } else {
                ScrollView {
                    LazyVStack(spacing: 1) {
                        ForEach(stateManager.sessions) { session in
                            Button {
                                withAnimation(.easeInOut(duration: 0.2)) { selectedSession = session }
                            } label: {
                                SessionRowView(session: session)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.vertical, 4)
                    .padding(.horizontal, 4)
                }
            }

            Spacer(minLength: 0)

            // Footer
            HStack {
                Text("CodePulse")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.quaternary)
                Spacer()
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
        }
    }

    private var emptyState: some View {
        VStack(spacing: 12) {
            Spacer()
            Image(systemName: "waveform.path")
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(.tertiary)
            Text("No active sessions")
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
            Text("Start a Claude Code session to see it here")
                .font(.system(size: 11))
                .foregroundStyle(.tertiary)
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }
}
