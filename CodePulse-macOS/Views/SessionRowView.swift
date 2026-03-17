import SwiftUI
import CodePulseShared

struct SessionRowView: View {
    let session: SessionState
    let onSelect: () -> Void
    @Environment(\.theme) private var theme
    @State private var isHovered = false
    @State private var isPressed = false

    var body: some View {
        Button(action: {
            withAnimation(.spring(duration: 0.15)) { isPressed = true }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) {
                withAnimation(.spring(duration: 0.15)) { isPressed = false }
            }
            onSelect()
        }) {
            HStack(spacing: 8) {
                SessionStatusView(
                    status: session.status,
                    completedTasks: session.completedTaskCount,
                    totalTasks: session.totalTaskCount
                )

                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 4) {
                        Text(session.summary)
                            .font(.system(size: 13, weight: isHovered ? .medium : .regular))
                            .foregroundStyle(theme.primaryText)
                            .lineLimit(1)
                            .truncationMode(.middle)
                        if !session.model.isEmpty {
                            SessionTagView(tag: session.model)
                        }
                    }
                    if let subtitle = session.lastEvent ?? session.currentTask {
                        Text(subtitle)
                            .font(.system(size: 11))
                            .foregroundStyle(session.status == .needsInput ? theme.warning : theme.secondaryText)
                            .lineLimit(1)
                    }
                }

                Spacer(minLength: 4)

                Text(relativeTime(session.startedAt))
                    .font(.system(size: 11, design: .rounded))
                    .foregroundStyle(theme.secondaryText)
            }
            .padding(.leading, 6)
            .padding(.trailing, 10)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: 5, style: .continuous)
                    .fill(isPressed ? theme.primaryText.opacity(0.1)
                          : isHovered ? theme.primaryText.opacity(0.06)
                          : Color.clear)
            )
            .scaleEffect(isPressed ? 0.98 : 1.0)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 2)
        .onHover { hovering in
            withAnimation(.easeOut(duration: 0.12)) { isHovered = hovering }
        }
    }

    private func relativeTime(_ date: Date) -> String {
        let seconds = -date.timeIntervalSinceNow
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m ago" }
        if seconds < 86400 { return "\(Int(seconds / 3600))h ago" }
        if seconds < 604800 { return "\(Int(seconds / 86400))d ago" }
        return "\(Int(seconds / 604800))w ago"
    }
}
