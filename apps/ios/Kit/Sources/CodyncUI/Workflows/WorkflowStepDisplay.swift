import CodyncKit
import SwiftUI

/// How a workflow's steps show their parents. Stored on this phone.
enum WorkflowStepDisplay: String {
    case labels
    case lines

    var symbol: String { self == .labels ? "list.bullet.rectangle" : "arrow.triangle.branch" }
    var title: String { self == .labels ? "Label mode" : "Line mode" }
    var detail: String {
        self == .labels ? "Show where each step's input comes from." : "Show the path with arrows."
    }
}

/// `From step {number}` for each step that feeds this one, side by side when they fit.
struct WorkflowSourceLine: View {
    var numbers: [Int]

    var body: some View {
        let labels = HStack(spacing: 12) { labelTexts }
        let stacked = VStack(alignment: .leading, spacing: 2) { labelTexts }
        ViewThatFits(in: .horizontal) {
            labels
            stacked
        }
    }

    @ViewBuilder private var labelTexts: some View {
        ForEach(numbers, id: \.self) { number in
            source(L10n.tr("From step {number}").replacingOccurrences(of: "{number}", with: "\(number)"))
        }
    }

    private func source(_ text: String) -> some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(Palette.tertiary)
            .lineLimit(1)
    }
}

/// The two ways to read a step's parents.
struct WorkflowDisplaySheet: View {
    var display: WorkflowStepDisplay
    var choose: (WorkflowStepDisplay) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ModalHeader("Dependency display")
            VStack(spacing: 10) {
                row(.labels)
                row(.lines)
            }
            .padding(.horizontal, 20)
            .padding(.bottom, 20)
        }
        .presentationDetents([.height(320)])
    }

    private func row(_ mode: WorkflowStepDisplay) -> some View {
        Button { choose(mode) } label: {
            HStack(spacing: 12) {
                Image(systemName: mode.symbol)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(Palette.text)
                    .frame(width: 36, height: 36)
                    .background(Palette.background, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                VStack(alignment: .leading, spacing: 2) {
                    Text(L10n.tr(mode.title))
                        .font(.body.weight(.medium))
                        .foregroundStyle(Palette.text)
                    Text(L10n.tr(mode.detail))
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .lineLimit(2)
                }
                Spacer(minLength: 8)
                if display == mode {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.body)
                        .foregroundStyle(Palette.text)
                }
            }
            .padding(14)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

/// Steps in rows, with an arrow from each parent down to the step it feeds.
struct WorkflowStepFlow: View {
    let graph: WorkflowGraph
    var schedules: [String: String]
    var botName: (String) -> String?

    var body: some View {
        let numbers = graph.stepNumbers()
        let ranks = graph.stepRanks()
        VStack(alignment: .leading, spacing: 36) {
            ForEach(Array(ranks.enumerated()), id: \.offset) { _, rank in
                HStack(alignment: .top, spacing: 10) {
                    ForEach(rank, id: \.self) { id in
                        if let node = graph.node(id) {
                            WorkflowNodeLine(
                                node: node,
                                schedule: schedules[node.id],
                                number: numbers[node.id],
                                parentNumbers: graph.parents(of: node.id).compactMap { numbers[$0] },
                                botName: botName
                            )
                            .padding(12)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                            .anchorPreference(key: StepAnchorsKey.self, value: .bounds) { [StepAnchor(id: id, anchor: $0)] }
                        }
                    }
                }
            }
        }
        .backgroundPreferenceValue(StepAnchorsKey.self) { anchors in
            GeometryReader { proxy in
                Canvas { context, _ in
                    stroke(&context, proxy, anchors, graph, numbers)
                }
                .allowsHitTesting(false)
            }
        }
    }

    private func stroke(_ context: inout GraphicsContext, _ proxy: GeometryProxy, _ anchors: [StepAnchor], _ graph: WorkflowGraph, _ numbers: [String: Int]) {
        let frames = Dictionary(anchors.map { ($0.id, proxy[$0.anchor]) }, uniquingKeysWith: { _, next in next })
        for node in graph.nodes {
            guard let child = frames[node.id] else { continue }
            for parent in graph.parents(of: node.id) where numbers[parent] != nil {
                guard let from = frames[parent] else { continue }
                let start = CGPoint(x: from.midX, y: from.maxY)
                let end = CGPoint(x: child.midX, y: child.minY)
                var path = Path()
                path.move(to: start)
                let mid = (start.y + end.y) / 2
                path.addCurve(to: end, control1: CGPoint(x: start.x, y: mid), control2: CGPoint(x: end.x, y: mid))
                context.stroke(path, with: .color(Palette.secondary.opacity(0.55)), lineWidth: 1.5)
                var arrow = Path()
                arrow.move(to: end)
                arrow.addLine(to: CGPoint(x: end.x - 5, y: end.y - 7))
                arrow.addLine(to: CGPoint(x: end.x + 5, y: end.y - 7))
                arrow.closeSubpath()
                context.fill(arrow, with: .color(Palette.secondary.opacity(0.55)))
            }
        }
    }
}

private struct StepAnchor {
    var id: String
    var anchor: Anchor<CGRect>
}

private struct StepAnchorsKey: PreferenceKey {
    static var defaultValue: [StepAnchor] { [] }
    static func reduce(value: inout [StepAnchor], nextValue: () -> [StepAnchor]) {
        value.append(contentsOf: nextValue())
    }
}
