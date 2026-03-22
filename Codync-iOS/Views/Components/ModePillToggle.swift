import SwiftUI
import CodyncShared

struct ModePillToggle: View {
    @Binding var mode: LiveActivityMode
    @Environment(\.theme) private var theme

    var body: some View {
        HStack(spacing: 0) {
            pillSegment(
                mode: .overall,
                icon: overallIcon,
                label: "Overall"
            )
            pillSegment(
                mode: .individual,
                icon: individualIcon,
                label: "Individual"
            )
        }
        .padding(3)
        .background(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(theme.primaryText.opacity(0.06))
        )
    }

    @ViewBuilder
    private func pillSegment(mode segmentMode: LiveActivityMode, icon: some View, label: String) -> some View {
        let isSelected = mode == segmentMode

        Button {
            withAnimation(.spring(duration: 0.35, bounce: 0.15)) {
                mode = segmentMode
            }
        } label: {
            HStack(spacing: 6) {
                icon
                Text(label)
                    .font(.system(size: 14, weight: isSelected ? .semibold : .regular))
            }
            .foregroundStyle(isSelected ? theme.primaryText : theme.tertiaryText)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(isSelected ? theme.primaryText.opacity(0.1) : .clear)
            )
        }
        .buttonStyle(.plain)
    }

    // 3-dot grid for Overall
    private var overallIcon: some View {
        Canvas { context, size in
            let fg = theme.primaryText
            let dotSize: CGFloat = 3.5
            let gap: CGFloat = 1.5
            let positions: [(CGFloat, CGFloat, Double)] = [
                (0, 0, 0.8), (dotSize + gap, 0, 0.5),
                (0, dotSize + gap, 0.3), (dotSize + gap, dotSize + gap, 0.15),
            ]
            for (x, y, opacity) in positions {
                let rect = CGRect(x: x + 1, y: y + 1, width: dotSize, height: dotSize)
                context.opacity = mode == .overall ? opacity : opacity * 0.5
                context.fill(
                    RoundedRectangle(cornerRadius: 1).path(in: rect),
                    with: .color(fg)
                )
            }
        }
        .frame(width: 12, height: 12)
    }

    // Single rect for Individual
    private var individualIcon: some View {
        RoundedRectangle(cornerRadius: 2.5, style: .continuous)
            .stroke(mode == .individual ? theme.primaryText : theme.tertiaryText, lineWidth: 1.5)
            .frame(width: 10, height: 10)
    }
}
