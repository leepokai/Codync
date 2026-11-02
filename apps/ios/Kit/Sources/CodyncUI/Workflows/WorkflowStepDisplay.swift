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

/// The source picker for one step: the current token, and each choice's token and label.
func workflowSourceChoices(graph: WorkflowGraph, step: String, botName: (String) -> String?) -> (selection: String, options: [(id: String, label: String)]) {
    let kinds = Dictionary(uniqueKeysWithValues: graph.nodes.map { ($0.id, $0.kind) })
    guard let kind = kinds[step], kind != .trigger, kind != .prompt else { return ("", []) }
    if let outline = WorkflowLayout.outline(of: graph) {
        let numbers = graph.stepNumbers()
        return workflowSourceChoices(outline: outline, kinds: kinds, step: step, triggerLabel: triggerLabel(graph.node(outline.trigger)),
                                     number: { numbers[$0] }, title: { id in
            graph.node(id).map { WorkflowCopy.heading($0, botName: botName) } ?? ""
        })
    }
    return fieldSourceChoices(graph: graph, step: step, botName: botName)
}

/// Choices for a draft that is not a saved graph yet. `*trigger` stands in for the trigger.
func workflowSourceChoices(outline: WorkflowOutline, kinds: [String: WorkflowNodeKind], step: String, triggerLabel: String,
                           number: (String) -> Int?, title: (String) -> String, branches: Bool = true) -> (selection: String, options: [(id: String, label: String)]) {
    guard let kind = kinds[step], kind != .trigger, kind != .prompt else { return ("", []) }
    let selection = outline.sourceTarget(of: step, kinds: kinds)?.token ?? ""
    var options: [(id: String, label: String)] = [(WorkflowOutline.SourceTarget.node(outline.trigger).token, triggerLabel)]
    let pool = kind == .condition ? outline.main : outline.main + outline.yes + outline.no + outline.after
    for id in pool where id != step {
        guard let item = kinds[id], item != .output, item != .prompt, item != .trigger, let place = number(id) else { continue }
        options.append((WorkflowOutline.SourceTarget.node(id).token, "\(place) · \(title(id))"))
    }
    if kind != .condition, outline.condition != nil, let condition = outline.condition, let place = number(condition) {
        if branches {
            options.append((WorkflowOutline.SourceTarget.yes.token, "\(place) · \(L10n.tr("Yes"))"))
            options.append((WorkflowOutline.SourceTarget.no.token, "\(place) · \(L10n.tr("No"))"))
        }
        options.append((WorkflowOutline.SourceTarget.both.token, "Both branches"))
    }
    return (selection, options)
}

private func fieldSourceChoices(graph: WorkflowGraph, step: String, botName: (String) -> String?) -> (selection: String, options: [(id: String, label: String)]) {
    let numbers = graph.stepNumbers()
    let trigger = graph.nodes.first { $0.kind == .trigger }
    var options: [(id: String, label: String)] = []
    if let trigger {
        options.append((trigger.id, triggerLabel(trigger)))
    }
    for node in graph.nodes where node.id != step && node.kind != .trigger && node.kind != .output && node.kind != .prompt && node.kind != .condition {
        guard let place = numbers[node.id] else { continue }
        options.append((node.id, "\(place) · \(WorkflowCopy.heading(node, botName: botName))"))
    }
    let parents = graph.parents(of: step).filter { graph.node($0)?.kind != .prompt }
    let selection: String
    if parents.count >= 2 {
        selection = WorkflowOutline.SourceTarget.both.token
    } else if let parent = parents.first, parent == graph.nodes.first(where: { $0.kind == .condition })?.id {
        let branch = graph.edges.first { $0.to == step && $0.from == parent }?.branch
        selection = branch == "no" ? WorkflowOutline.SourceTarget.no.token : WorkflowOutline.SourceTarget.yes.token
    } else {
        selection = parents.first ?? ""
    }
    if let condition = graph.nodes.first(where: { $0.kind == .condition }), let place = numbers[condition.id] {
        func offer(_ branch: String, _ token: String, _ label: String) {
            let owner = graph.edges.first { $0.from == condition.id && $0.branch == branch }?.to
            if owner == nil || owner == step || selection == token {
                options.append((token, label))
            }
        }
        offer("yes", WorkflowOutline.SourceTarget.yes.token, "\(place) · \(L10n.tr("Yes"))")
        offer("no", WorkflowOutline.SourceTarget.no.token, "\(place) · \(L10n.tr("No"))")
        if WorkflowOutline.rewired(graph, step: step, to: .both) != nil || selection == WorkflowOutline.SourceTarget.both.token {
            options.append((WorkflowOutline.SourceTarget.both.token, "Both branches"))
        }
    }
    return (selection, options)
}

private func triggerLabel(_ node: WorkflowNode?) -> String {
    switch node?.string("trigger") {
    case "schedule": "Schedule"
    case "webhook": "Webhook"
    default: "Manual"
    }
}

/// `From step {number}` and `From {name}`, side by side when they fit.
struct WorkflowSourceLine: View {
    var numbers: [Int] = []
    var names: [String] = []

    var body: some View {
        let labels = HStack(spacing: 12) { labelTexts }
        let stacked = VStack(alignment: .leading, spacing: 2) { labelTexts }
        ViewThatFits(in: .horizontal) {
            labels
            stacked
        }
    }

    @ViewBuilder private var labelTexts: some View {
        ForEach(Array(names.enumerated()), id: \.offset) { _, name in
            source(L10n.tr("From {name}").replacingOccurrences(of: "{name}", with: L10n.tr(name)))
        }
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
