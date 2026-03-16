import SwiftUI
import CodePulseShared

struct SessionRowView: View {
    let session: SessionState
    @State private var isHovered = false

    var body: some View {
        HStack(spacing: 10) {
            StatusDotView(status: session.status)

            VStack(alignment: .leading, spacing: 1) {
                Text(session.summary)
                    .font(.system(size: 13, weight: isHovered ? .medium : .regular))
                    .foregroundStyle(.primary)
                    .lineLimit(1)

                // Show lastEvent or status subtitle
                if let subtitle = subtitleText {
                    Text(subtitle)
                        .font(.system(size: 11))
                        .foregroundStyle(subtitleColor)
                        .lineLimit(1)
                }
            }

            Spacer(minLength: 4)

            Text(relativeTime(session.startedAt))
                .font(.system(size: 11, design: .rounded))
                .foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(isHovered ? Color.primary.opacity(0.06) : Color.clear)
        )
        .onHover { h in withAnimation(.easeOut(duration: 0.12)) { isHovered = h } }
        .contentShape(Rectangle())
    }

    private var subtitleText: String? {
        if let lastEvent = session.lastEvent {
            return lastEvent
        }
        if let currentTask = session.currentTask {
            return currentTask
        }
        return nil
    }

    private var subtitleColor: Color {
        switch session.status {
        case .working: return .green
        case .needsInput: return .orange
        case .error: return .red
        default: return .secondary
        }
    }

    private func relativeTime(_ date: Date) -> String {
        let seconds = -date.timeIntervalSinceNow
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m" }
        if seconds < 86400 { return "\(Int(seconds / 3600))h" }
        return "\(Int(seconds / 86400))d"
    }
}
