import SwiftUI
import CodyncShared

struct LiveActivityPreview: View {
    let mode: LiveActivityMode
    var sessions: [SessionState]?
    var primarySessionId: String?
    var maxSessions: Int = 4
    @Environment(\.theme) private var theme

    private var displaySessions: [SessionState] {
        if let sessions, !sessions.isEmpty { return Array(sessions.prefix(maxSessions)) }
        return Array(Self.mockSessions.prefix(maxSessions))
    }

    var body: some View {
        ZStack {
            // Subtle top glow
            RadialGradient(
                colors: [theme.primaryText.opacity(0.04), .clear],
                center: .top,
                startRadius: 0,
                endRadius: 60
            )
            .frame(height: 40)
            .frame(maxHeight: .infinity, alignment: .top)

            Group {
                if mode == .overall {
                    overallPreview
                } else {
                    individualPreview
                }
            }
            .transition(.opacity.combined(with: .scale(scale: 0.96)))
        }
        .padding(16)
        .background(
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .fill(Color.black.opacity(0.6))
                .overlay(
                    RoundedRectangle(cornerRadius: 20, style: .continuous)
                        .stroke(theme.primaryText.opacity(0.06), lineWidth: 1)
                )
        )
        .animation(.easeInOut(duration: 0.3), value: displaySessions.map(\.sessionId))
        .animation(.easeInOut(duration: 0.3), value: displaySessions.map(\.currentTask))
    }

    // MARK: - Overall Preview

    @ViewBuilder
    private var overallPreview: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(Array(displaySessions.enumerated()), id: \.element.sessionId) { index, session in
                let isPrimary = primarySessionId != nil
                    ? session.sessionId == primarySessionId
                    : index == 0

                OverallSessionRow(
                    projectName: session.projectName,
                    model: session.model,
                    currentTask: session.currentTask,
                    status: session.status,
                    isPrimary: isPrimary,
                    fg: theme.primaryText
                )
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .background(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .fill(isPrimary ? theme.primaryText.opacity(0.06) : .clear)
                )
            }
        }
    }

    // MARK: - Individual Preview

    @ViewBuilder
    private var individualPreview: some View {
        VStack(spacing: 8) {
            ForEach(Array(displaySessions.enumerated()), id: \.element.sessionId) { index, session in
                let isPrimary = primarySessionId != nil
                    ? session.sessionId == primarySessionId
                    : index == 0

                individualCard(session: session, isPrimary: isPrimary)
            }
        }
    }

    @ViewBuilder
    private func individualCard(session: SessionState, isPrimary: Bool) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(spacing: 6) {
                Image(systemName: "chevron.left.forwardslash.chevron.right")
                    .font(.system(size: 7, weight: .bold))
                    .foregroundStyle(theme.primaryText.opacity(0.5))
                    .frame(width: 15, height: 15)
                    .background(Circle().fill(theme.primaryText.opacity(0.08)))

                Text(session.projectName)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(theme.primaryText.opacity(0.72))
                    .lineLimit(1)

                Spacer()

                if session.costUSD > 0 {
                    Text(String(format: "$%.2f", session.costUSD))
                        .font(.system(size: 10))
                        .foregroundStyle(theme.primaryText.opacity(0.5))
                        .monospacedDigit()
                }
            }

            Spacer().frame(height: 6)

            // Mini card stack
            ZStack {
                miniCard(text: "Searching code", isBehind: true)
                miniCard(text: session.currentTask ?? "Reading file", isBehind: false)
            }
            .frame(height: 36)

            Spacer().frame(height: 6)

            // Footer
            HStack(spacing: 4) {
                Image(systemName: "arrow.turn.down.right")
                    .font(.system(size: 8, weight: .semibold))
                    .foregroundStyle(theme.primaryText.opacity(0.4))
                    .frame(width: 14, height: 14)
                Text(session.currentTask ?? "Working…")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(theme.primaryText)
                    .lineLimit(1)
                    .id(session.currentTask)
                    .transition(.push(from: .bottom))

                Spacer()

                Text("02:34")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(theme.primaryText.opacity(0.4))
                    .monospacedDigit()
            }
        }
        .padding(10)
        .background(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(isPrimary ? theme.primaryText.opacity(0.06) : .clear)
        )
    }

    // MARK: - Helpers

    @ViewBuilder
    private func miniCard(text: String, isBehind: Bool) -> some View {
        HStack(spacing: 6) {
            Image(systemName: "checkmark")
                .font(.system(size: 8, weight: .semibold))
                .foregroundStyle(theme.primaryText.opacity(0.4))
                .frame(width: 14, height: 14)
                .background(Circle().fill(theme.primaryText.opacity(0.08)))

            Text(text)
                .font(.system(size: 10))
                .foregroundStyle(isBehind ? theme.primaryText.opacity(0.4) : theme.primaryText)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: isBehind ? 6 : 10, style: .continuous)
                .fill(isBehind
                      ? theme.primaryText.opacity(0.03)
                      : Color(red: 0.14, green: 0.14, blue: 0.16))
        )
        .scaleEffect(isBehind ? 0.92 : 1)
        .offset(y: isBehind ? 6 : 0)
        .opacity(isBehind ? 0.5 : 1)
        .zIndex(isBehind ? 0 : 1)
    }

    // MARK: - Mock Data

    static let mockSessions: [SessionState] = [
        SessionState(
            sessionId: "mock-1", projectName: "Codync", gitBranch: "main",
            status: .working, model: "claude-opus-4-6", summary: "Building UI",
            currentTask: "Reading file", costUSD: 0.42
        ),
        SessionState(
            sessionId: "mock-2", projectName: "MyApp", gitBranch: "feat/auth",
            status: .idle, model: "claude-sonnet-4-6", summary: "Auth flow",
            currentTask: "Editing code", costUSD: 0.18
        ),
        SessionState(
            sessionId: "mock-3", projectName: "Backend", gitBranch: "main",
            status: .idle, model: "claude-haiku-4-5", summary: "API work",
            costUSD: 0.05
        ),
    ]
}
