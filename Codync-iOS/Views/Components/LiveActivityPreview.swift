import SwiftUI
import CodyncShared

struct LiveActivityPreview: View {
    let mode: LiveActivityMode
    var sessions: [SessionState]?
    @Environment(\.theme) private var theme

    private var displaySessions: [SessionState] {
        if let sessions, !sessions.isEmpty { return Array(sessions.prefix(4)) }
        return Self.mockSessions
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
    }

    // MARK: - Overall Preview

    @ViewBuilder
    private var overallPreview: some View {
        VStack(spacing: 10) {
            ForEach(Array(displaySessions.enumerated()), id: \.offset) { index, session in
                HStack(spacing: 8) {
                    Circle()
                        .fill(dotColor(for: session.status, isFirst: index == 0))
                        .frame(width: 7, height: 7)

                    Text(session.projectName)
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(theme.primaryText)
                        .lineLimit(1)

                    Text(modelLabel(session.model))
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(theme.primaryText.opacity(0.5))
                        .padding(.horizontal, 7)
                        .padding(.vertical, 2)
                        .background(
                            Capsule().fill(theme.primaryText.opacity(0.08))
                        )

                    if let task = session.currentTask, !task.isEmpty {
                        Spacer(minLength: 0)
                        Text(task)
                            .font(.system(size: 11))
                            .foregroundStyle(theme.primaryText.opacity(0.3))
                            .lineLimit(1)
                    }
                }
            }
        }
    }

    // MARK: - Individual Preview

    @ViewBuilder
    private var individualPreview: some View {
        let session = displaySessions.first ?? Self.mockSessions[0]

        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(spacing: 8) {
                Image(systemName: "chevron.left.forwardslash.chevron.right")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(theme.primaryText.opacity(0.6))
                    .frame(width: 18, height: 18)
                    .background(Circle().fill(theme.primaryText.opacity(0.08)))

                Text(session.projectName)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(theme.primaryText.opacity(0.72))

                Spacer()

                Text(String(format: "$%.2f", session.costUSD))
                    .font(.system(size: 13))
                    .foregroundStyle(theme.primaryText.opacity(0.6))
            }

            Spacer().frame(height: 10)

            // Card stack
            ZStack {
                // Behind card
                mockCard(text: "Searching code", icon: "checkmark", isBehind: true)
                // Front card
                mockCard(text: "Reading AppDelegate.swift", icon: "checkmark", isBehind: false)
            }
            .frame(height: 52)

            Spacer().frame(height: 10)

            // Footer
            HStack(spacing: 6) {
                Image(systemName: "pencil.line")
                    .font(.system(size: 10, weight: .semibold))
                    .foregroundStyle(theme.primaryText)
                    .frame(width: 20, height: 18)
                Text("Editing ViewModel.swift")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(theme.primaryText)
                    .lineLimit(1)

                Spacer()

                Text("02:34")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(theme.primaryText.opacity(0.5))
                    .monospacedDigit()
            }
        }
    }

    // MARK: - Helpers

    @ViewBuilder
    private func mockCard(text: String, icon: String, isBehind: Bool) -> some View {
        HStack(spacing: 8) {
            Image(systemName: icon)
                .font(.system(size: 9, weight: .semibold))
                .foregroundStyle(theme.primaryText.opacity(0.5))
                .frame(width: 18, height: 18)
                .background(Circle().fill(theme.primaryText.opacity(0.08)))

            Text(text)
                .font(.system(size: 12))
                .foregroundStyle(isBehind ? theme.primaryText.opacity(0.5) : theme.primaryText)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: isBehind ? 8 : 12, style: .continuous)
                .fill(theme.primaryText.opacity(isBehind ? 0.04 : 0.08))
        )
        .scaleEffect(isBehind ? 0.9 : 1)
        .offset(y: isBehind ? 8 : 0)
        .opacity(isBehind ? 0.6 : 1)
        .zIndex(isBehind ? 0 : 1)
    }

    private func dotColor(for status: SessionStatus, isFirst: Bool) -> Color {
        if isFirst { return theme.primaryText }
        switch status {
        case .working: return theme.primaryText
        case .idle: return theme.primaryText.opacity(0.3)
        default: return theme.primaryText.opacity(0.5)
        }
    }

    private func modelLabel(_ model: String) -> String {
        let lower = model.lowercased()
        if lower.contains("opus") { return "Opus" }
        if lower.contains("sonnet") { return "Sonnet" }
        if lower.contains("haiku") { return "Haiku" }
        return model
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
