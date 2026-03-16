import SwiftUI
import CodePulseShared

struct SessionListView: View {
    @ObservedObject var stateManager: SessionStateManager
    @State private var selectedSession: SessionState?

    var body: some View {
        VStack(spacing: 0) {
            if let session = selectedSession {
                SessionDetailView(session: session) {
                    withAnimation { selectedSession = nil }
                }
            } else {
                sessionList
            }

            Divider().padding(.horizontal, 8)
            footer
        }
        .frame(width: 340)
    }

    private var sessionList: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                if stateManager.sessions.isEmpty {
                    emptyState
                } else {
                    ForEach(stateManager.sessions) { session in
                        Button {
                            withAnimation { selectedSession = session }
                        } label: {
                            SessionRowView(session: session)
                        }
                        .buttonStyle(.plain)

                        if session.id != stateManager.sessions.last?.id {
                            Divider().padding(.horizontal, 10)
                        }
                    }
                }
            }
            .padding(.vertical, 4)
        }
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Image(systemName: "waveform.path")
                .font(.system(size: 32))
                .foregroundStyle(.tertiary)
            Text("No active Claude Code sessions")
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, minHeight: 120)
    }

    private var footer: some View {
        HStack {
            Text("CodePulse")
                .font(.system(size: 10))
                .foregroundStyle(.quaternary)
            Spacer()
            Text("⌘.")
                .font(.system(size: 10))
                .foregroundStyle(.quaternary)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }
}
