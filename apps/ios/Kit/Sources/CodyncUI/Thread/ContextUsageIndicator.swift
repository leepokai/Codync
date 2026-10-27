import CodyncKit
import SwiftUI

/// The main agent session's last reported context use, independent of voice and billing usage.
struct ContextUsageIndicator: View {
    let usage: ContextUsage?
    let diameter: CGFloat
    @State private var showingDetails = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var ink: Color {
        switch usage?.level {
        case "warning": Palette.attention
        case "danger": Palette.danger
        default: Palette.secondary
        }
    }

    private var label: String {
        guard let usage else { return L10n.tr("Context usage unavailable") }
        return L10n.tr("Context window: {percent}% used")
            .replacingOccurrences(of: "{percent}", with: String(usage.percent))
    }

    var body: some View {
        Button {
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { showingDetails = true }
        } label: {
            ZStack {
                Circle().strokeBorder(Palette.tertiary.opacity(0.35),
                                style: StrokeStyle(lineWidth: 2, dash: usage == nil ? [2, 3] : []))
                if let usage {
                    Circle().inset(by: 1).trim(from: 0, to: CGFloat(usage.percent) / 100)
                        .stroke(ink, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                        .rotationEffect(.degrees(-90))
                }
                Text(usage.map { "\($0.percent)%" } ?? "–")
                    .font(.system(size: 8, weight: .medium, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(ink)
            }
            .frame(width: diameter, height: diameter)
            .frame(width: 44, height: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScale())
        .accessibilityLabel(label)
        .help(label)
        .popover(isPresented: $showingDetails, attachmentAnchor: .rect(.bounds), arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 10) {
                Text(L10n.tr("Context window"))
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(Palette.text)
                if let usage {
                    Text("\(usage.percent)%")
                        .font(.title2.weight(.semibold))
                        .monospacedDigit()
                        .foregroundStyle(ink)
                    Text(L10n.tr("{used} / {max} tokens")
                        .replacingOccurrences(of: "{used}", with: usage.usedTokens.formatted())
                        .replacingOccurrences(of: "{max}", with: usage.maxTokens.formatted()))
                        .font(.subheadline)
                        .foregroundStyle(Palette.text)
                    Text(L10n.tr("Last reported by the agent."))
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                } else {
                    Text(L10n.tr("The agent has not reported this session's context usage yet."))
                        .font(.footnote)
                        .foregroundStyle(Palette.secondary)
                }
            }
            .padding(16)
            .frame(width: 250, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            .presentationCompactAdaptation(.popover)
        }
    }
}
