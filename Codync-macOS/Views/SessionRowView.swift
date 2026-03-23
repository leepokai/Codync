import SwiftUI
import CodyncShared

struct SessionRowView: View {
    let session: SessionState
    var isPrimary: Bool = false
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
                    totalTasks: session.totalTaskCount,
                    waitingReason: session.waitingReason
                )

                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 4) {
                        Text(session.projectName)
                            .font(.system(size: 13, weight: isHovered ? .medium : .regular))
                            .foregroundStyle(theme.primaryText)
                            .lineLimit(1)
                        if !session.model.isEmpty {
                            SessionTagView(tag: session.model)
                        }
                    }
                    if let desc = session.statusDescription {
                        Text(desc)
                            .font(.system(size: 11))
                            .foregroundStyle(subtitleColor)
                            .lineLimit(1)
                    }
                }

                Spacer(minLength: 4)

                Text(relativeTime(session.updatedAt))
                    .font(.system(size: 11, design: .monospaced))
                    .foregroundStyle(theme.tertiaryText)
            }
            .padding(.leading, 6)
            .padding(.trailing, 10)
            .padding(.vertical, 5)
            .background(
                RoundedRectangle(cornerRadius: 5, style: .continuous)
                    .fill(isPressed ? theme.pressedBackground
                          : isHovered ? theme.hoverBackground
                          : isPrimary ? theme.primaryText.opacity(0.06)
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

    private var subtitleColor: Color {
        guard session.status == .needsInput else { return theme.secondaryText }
        return theme.waitingColor(for: session.waitingReason)
    }

    private func relativeTime(_ date: Date) -> String {
        let seconds = -date.timeIntervalSinceNow
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m" }
        if seconds < 86400 { return "\(Int(seconds / 3600))h" }
        if seconds < 604800 { return "\(Int(seconds / 86400))d" }
        return "\(Int(seconds / 604800))w"
    }
}
