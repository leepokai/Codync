import CodyncKit
import SwiftUI

/// Name, trigger and steps, then one save. The host still owns the saved graph.
struct WorkflowNewSheet: View {
    var create: (String, WorkflowGraph) async -> String?

    @Environment(BotStore.self) private var model
    @Environment(\.dismissModal) private var dismiss
    @State private var name = ""
    @State private var trigger = "manual"
    @State private var steps: [WorkflowNode] = []
    @State private var pickingTrigger = false
    @State private var adding = false
    @State private var editing: DraftStep?
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        VStack(spacing: 0) {
            ModalHeader("New workflow") {
                IconButton("Save", systemImage: "checkmark") { Task { await save() } }
                    .disabled(saving)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    if let error {
                        Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
                    }
                    labeled("Name") {
                        TextField(L10n.tr("Name"), text: $name)
                            .font(.body)
                            .workflowInput()
                    }
                    labeled("Triggers") {
                        Button { pickingTrigger = true } label: {
                            choice(WorkflowCopy.symbol(.trigger), title: triggerTitle, detail: triggerDetail, showsChevron: true)
                        }
                        .buttonStyle(.plain)
                    }
                    labeled("Steps") {
                        VStack(spacing: 10) {
                            ForEach(steps, id: \.id) { node in
                                HStack(spacing: 8) {
                                    Button { editing = DraftStep(id: node.id) } label: {
                                        choice(WorkflowCopy.symbol(node.kind), title: rowTitle(node), detail: rowDetail(node), showsChevron: true)
                                    }
                                    .buttonStyle(.plain)
                                    IconButton("Delete", systemImage: "trash") { steps.removeAll { $0.id == node.id } }
                                }
                            }
                            WorkflowAddStepButton { adding = true }
                        }
                    }
                }
                .padding(.horizontal, 20)
                .padding(.bottom, 24)
            }
        }
        .background(Palette.background)
        .codyncSheet(isPresented: $pickingTrigger) {
            WorkflowTriggerSheet(selection: trigger) { trigger = $0 }
        }
        .codyncSheet(isPresented: $adding) {
            WorkflowAddStepSheet(draftHasCondition: steps.contains { $0.kind == .condition }) { kind in
                steps.append(WorkflowNode.make(kind))
                adding = false
            }
        }
        .codyncSheet(item: $editing) { item in
            if let node = steps.first(where: { $0.id == item.id }) {
                let choices = draftChoices(node.id)
                WorkflowStepSheet(node: node, webhook: nil, onChange: { edit in
                    if let index = steps.firstIndex(where: { $0.id == edit.node.id }) {
                        steps[index] = edit.node
                    }
                }, onReveal: { [] }, onRotate: { nil }, workflowId: "", sourceSelection: choices.selection,
                   sourceOptions: choices.options, onSource: { retargetDraft(node.id, to: $0) })
            }
        }
    }

    private func labeled(_ title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.tr(title))
                .font(.caption)
                .foregroundStyle(Palette.secondary)
            content()
        }
    }

    private func choice(_ symbol: String, title: String, detail: String, showsChevron: Bool) -> some View {
        WorkflowChoiceCard(symbol: symbol, title: title, detail: detail, showsChevron: showsChevron)
    }

    private var triggerTitle: String {
        switch trigger {
        case "schedule": "Schedule"
        case "webhook": "Webhook"
        default: "Manual"
        }
    }

    private var triggerDetail: String {
        switch trigger {
        case "schedule": "Runs at the time you set."
        case "webhook": "Runs when its address receives a request."
        default: "Runs when you start it."
        }
    }

    private func rowTitle(_ node: WorkflowNode) -> String {
        let kind = kindTitle(node.kind)
        guard let name = WorkflowCopy.who(node, botName: { model.bots[$0]?.name }) else { return kind }
        return "\(L10n.tr(kind)) · \(name)"
    }

    /// The explanation until this step has something of its own, then that text.
    private func rowDetail(_ node: WorkflowNode) -> String {
        let written = WorkflowCopy.summary(node, botName: { model.bots[$0]?.name }, schedule: nil)
        switch node.kind {
        case .prompt:
            return blank(node, "text") ? "Fixed text for the next step." : written
        case .agent:
            return blank(node, "task") && blank(node, "botId") ? "An agent finishes this step with its tools." : written
        case .approval:
            return blank(node, "question") ? "The run waits for you to decide." : written
        case .output:
            return node.string("target") == "chat" ? written : "Sends the result to you."
        case .condition:
            let op = node.string("op")
            return (op.isEmpty || op == "contains") && blank(node, "value") ? "A yes or no that splits into two paths." : written
        default:
            return written
        }
    }

    /// The trigger is `*trigger` until the workflow is saved.
    private func draftOutline() -> (WorkflowOutline, [String: WorkflowNodeKind]) {
        var main: [String] = []
        var condition: String?
        var after: [String] = []
        var kinds: [String: WorkflowNodeKind] = ["*trigger": .trigger]
        for node in steps {
            kinds[node.id] = node.kind
            if node.kind == .condition, condition == nil {
                condition = node.id
            } else if condition == nil {
                main.append(node.id)
            } else {
                after.append(node.id)
            }
        }
        return (WorkflowOutline(trigger: "*trigger", main: main, condition: condition, after: after), kinds)
    }

    private func draftChoices(_ id: String) -> (selection: String, options: [(id: String, label: String)]) {
        let (outline, kinds) = draftOutline()
        let numbers = Dictionary(uniqueKeysWithValues: steps.enumerated().map { ($1.id, $0 + 1) })
        return workflowSourceChoices(outline: outline, kinds: kinds, step: id, triggerLabel: triggerTitle, number: { numbers[$0] },
                                     title: { id in
            steps.first { $0.id == id }.map { WorkflowCopy.heading($0, botName: { model.bots[$0]?.name }) } ?? ""
        }, branches: false)
    }

    private func retargetDraft(_ id: String, to token: String) {
        let (outline, kinds) = draftOutline()
        let target = WorkflowOutline.SourceTarget(token: token)
        guard outline.sourceTarget(of: id, kinds: kinds) != target, let next = outline.retargeting(id, to: target) else { return }
        let order = next.main + (next.condition.map { [$0] } ?? []) + next.yes + next.no + next.after
        let byId = Dictionary(uniqueKeysWithValues: steps.map { ($0.id, $0) })
        steps = order.compactMap { byId[$0] }
    }

    private func blank(_ node: WorkflowNode, _ key: String) -> Bool {
        node.string(key).trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private func kindTitle(_ kind: WorkflowNodeKind) -> String {
        switch kind {
        case .agent: "Agent"
        case .condition: "Condition"
        case .approval: "Approval"
        case .prompt: "Prompt"
        case .output: "Output"
        default: "Note"
        }
    }

    private func save() async {
        guard !saving else { return }
        saving = true
        defer { saving = false }
        let trimmed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        let message = await create(trimmed.isEmpty ? "Workflow" : trimmed, composed())
        if let message {
            error = message
        } else {
            dismiss()
        }
    }

    /// A straight outline: the chosen trigger, then the steps. One condition sends the steps after it down both paths.
    private func composed() -> WorkflowGraph {
        var triggerNode = WorkflowNode.make(.trigger)
        triggerNode.setString("trigger", trigger)
        if trigger == "schedule" {
            let zone = TimeZone.current.identifier
            triggerNode.schedule = RoutineTrigger(type: "cron", expression: "0 9 * * *", timeZone: zone)
            triggerNode.setString("timezone", zone)
        }
        var nodes = [triggerNode]
        var main: [String] = []
        var condition: String?
        var after: [String] = []
        for node in steps {
            nodes.append(node)
            if node.kind == .condition, condition == nil {
                condition = node.id
            } else if condition == nil {
                main.append(node.id)
            } else {
                after.append(node.id)
            }
        }
        let outline = WorkflowOutline(trigger: triggerNode.id, main: main, condition: condition, after: after)
        return WorkflowLayout.graph(from: outline, keeping: WorkflowGraph(nodes: nodes, edges: []))
    }
}

/// The step open for editing. The node itself stays in `steps`, so this only carries its id.
private struct DraftStep: Identifiable {
    var id: String
}

/// Manual, schedule or webhook. A schedule starts at 09:00 every day; the editor changes the time.
private struct WorkflowTriggerSheet: View {
    var selection: String
    var choose: (String) -> Void
    @Environment(\.dismissModal) private var dismiss

    private let rows: [(String, String, String)] = [
        ("manual", "Manual", "Runs when you start it."),
        ("schedule", "Schedule", "Runs at the time you set."),
        ("webhook", "Webhook", "Runs when its address receives a request."),
    ]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ModalHeader("Triggers")
            Text(L10n.tr("How it starts"))
                .font(.caption)
                .foregroundStyle(Palette.secondary)
                .padding(.horizontal, 20)
            VStack(spacing: 0) {
                ForEach(rows, id: \.0) { id, title, detail in
                    Button {
                        choose(id)
                        dismiss()
                    } label: {
                        HStack(alignment: .center, spacing: 12) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(L10n.tr(title))
                                    .font(.body.weight(.medium))
                                    .foregroundStyle(Palette.text)
                                Text(L10n.tr(detail))
                                    .font(.caption)
                                    .foregroundStyle(Palette.secondary)
                                    .lineLimit(2)
                            }
                            Spacer(minLength: 8)
                            if selection == id {
                                Image(systemName: "checkmark")
                                    .font(.body.weight(.semibold))
                                    .foregroundStyle(Palette.text)
                            }
                        }
                        .padding(.vertical, 14)
                        .padding(.horizontal, 16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .padding(.horizontal, 20)
            .padding(.bottom, 20)
        }
        .background(Palette.background)
        .presentationDetents([.height(360)])
    }
}
