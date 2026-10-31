import CodyncKit
import SwiftUI

/// Outline columns, or each step's settings when the graph is not an outline.
struct WorkflowEditorView: View {
    let workflowId: String

    @Environment(BotStore.self) private var model
    @State private var title = ""
    @State private var graph = WorkflowGraph(nodes: [], edges: [])
    @State private var outline: WorkflowOutline?
    @State private var mode: WorkflowEditMode = .readOnly
    @State private var baseRevision: Int64 = 0
    @State private var webhooks: [WorkflowWebhook] = []
    @State private var scheduleLines: [String: String] = [:]
    @State private var loaded = false
    @State private var dirty = false
    @State private var saving = false
    @State private var pending = false
    @State private var error: String?
    @State private var editing: StepRoute?
    @State private var adding: AddRoute?
    @State private var saveTask: Task<Void, Never>?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                if model.deletedWorkflows.contains(workflowId) {
                    Text(L10n.tr("This workflow was deleted"))
                        .font(.body)
                        .foregroundStyle(Palette.secondary)
                } else if !loaded {
                    Spinner()
                } else {
                    notices
                    switch mode {
                    case .readOnly:
                        Text(L10n.tr("Update Codync on this computer to edit this workflow."))
                            .font(.callout)
                            .foregroundStyle(Palette.secondary)
                        stepList(editable: false)
                    case .fields:
                        Text(L10n.tr("This workflow was arranged on a computer. You can edit each step's settings."))
                            .font(.callout)
                            .foregroundStyle(Palette.secondary)
                        stepList(editable: true)
                    case .outline:
                        outlineColumns
                    case .script:
                        Text(L10n.tr("This workflow is a script. You can change its trigger. The steps appear when it runs."))
                            .font(.callout)
                            .foregroundStyle(Palette.secondary)
                        stepList(editable: true)
                    }
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Palette.background)
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.large)
        .codyncSheet(item: $editing) { route in
            if let node = graph.node(route.id) {
                WorkflowStepSheet(
                    node: node,
                    webhook: webhooks.first { $0.nodeId == route.id },
                    onChange: { replace($0) },
                    onReveal: { await revealWebhook() },
                    onRotate: { await rotateWebhook(route.id) }
                )
            }
        }
        .codyncSheet(item: $adding) { route in
            WorkflowAddStepSheet(route: route, outline: outline) { kind in
                add(kind, route: route)
                adding = nil
            }
        }
        .task { await load() }
        .onChange(of: model.workflowReload) { _, _ in
            guard loaded, !dirty, !saving else { return }
            Task { await load() }
        }
        .onChange(of: model.deletedWorkflows) { _, gone in
            if gone.contains(workflowId) { stopEditing() }
        }
        .onDisappear { flush() }
        .tracked(.screenViewed(.workflowEditor))
    }

    @ViewBuilder private var notices: some View {
        if let notice = model.workflowNotices[workflowId] {
            Text(L10n.tr(notice)).font(.callout).foregroundStyle(Palette.secondary)
        }
        if let error {
            Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
        }
    }

    private var outlineColumns: some View {
        VStack(alignment: .leading, spacing: 16) {
            if let outline {
                column(L10n.tr("Triggers"), ids: [outline.trigger], allowAdd: false)
                column(L10n.tr("Steps"), ids: outline.main, allowAdd: true, add: .init(column: .main))
                if let condition = outline.condition {
                    column(L10n.tr("Condition"), ids: [condition], allowAdd: false)
                    column(L10n.tr("Yes"), ids: outline.yes, allowAdd: true, add: .init(column: .yes))
                    column(L10n.tr("No"), ids: outline.no, allowAdd: true, add: .init(column: .no))
                    column(L10n.tr("Then"), ids: outline.after, allowAdd: true, add: .init(column: .after))
                }
                if !outline.loose.isEmpty {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(L10n.tr("Steps")).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.secondary)
                        ForEach(outline.loose, id: \.self) { id in
                            if let node = graph.node(id) {
                                looseRow(node)
                            }
                        }
                    }
                }
            }
        }
    }

    private func column(_ title: String, ids: [String], allowAdd: Bool, add: AddRoute? = nil) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.secondary)
            ForEach(ids, id: \.self) { id in
                if let node = graph.node(id) {
                    stepRow(node, menu: true)
                }
            }
            if allowAdd, ids.isEmpty, let add {
                Button(L10n.tr("Add step")) { adding = add }
                    .font(.callout)
                    .buttonStyle(.plain)
                    .foregroundStyle(Palette.secondary)
            }
        }
    }

    private func stepList(editable: Bool) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(WorkflowLayout.readingOrder(of: graph), id: \.self) { id in
                if let node = graph.node(id) {
                    stepRow(node, menu: false)
                        .allowsHitTesting(editable)
                }
            }
        }
    }

    private func stepRow(_ node: WorkflowNode, menu: Bool) -> some View {
        let row = Button { editing = StepRoute(id: node.id) } label: {
            WorkflowNodeLine(
                node: node,
                schedule: scheduleLines[node.id],
                caption: dangling(node) ? "This prompt doesn't feed a step" : nil,
                botName: botName
            )
        }
        .buttonStyle(.plain)
        return Group {
            if menu, mode == .outline {
                row.contextActions { rowMenu(node) }
            } else {
                row
            }
        }
    }

    private func looseRow(_ node: WorkflowNode) -> some View {
        HStack(alignment: .center, spacing: 8) {
            WorkflowNodeLine(node: node, botName: botName)
            Button(L10n.tr("Add to steps")) { adopt(node.id) }
                .font(.callout)
                .buttonStyle(.plain)
                .foregroundStyle(Palette.secondary)
        }
    }

    private func rowMenu(_ node: WorkflowNode) -> [MenuItem] {
        guard let outline else { return [] }
        var items: [MenuItem] = []
        if let (column, index) = locate(node.id, outline) {
            let list = outline[keyPath: column.path]
            if index > 0 {
                items.append(MenuItem("Move up", icon: "arrow.up") { move(node.id, by: -1) })
            }
            if index + 1 < list.count {
                items.append(MenuItem("Move down", icon: "arrow.down") { move(node.id, by: 1) })
            }
        }
        if node.kind != .trigger && node.kind != .condition {
            items.append(MenuItem("Duplicate", icon: "plus.square.on.square") { duplicate(node.id) })
        }
        items.append(MenuItem("Add step", icon: "plus") { adding = AddRoute(after: node.id) })
        if node.kind != .trigger {
            items.append(MenuItem("Delete", icon: "trash", destructive: true) { delete(node.id) })
        }
        return items
    }

    private func dangling(_ node: WorkflowNode) -> Bool {
        guard node.kind == .prompt else { return false }
        return !graph.edges.contains { $0.from == node.id }
    }

    private func botName(_ id: String) -> String? { model.bots[id]?.name }

    private func load() async {
        guard let client = model.client, !model.deletedWorkflows.contains(workflowId) else { return }
        do {
            let detail = try await client.workflow(workflowId)
            guard !dirty else { return }
            apply(detail, conflict: false)
            loaded = true
            error = nil
            await loadSchedules()
        } catch {
            if !loaded { self.error = error.localizedDescription }
        }
    }

    private func apply(_ detail: WorkflowDetail, conflict: Bool) {
        title = detail.title
        graph = detail.graph
        baseRevision = detail.revision
        webhooks = detail.webhooks
        mode = detail.resolvedEditMode
        if mode == .outline {
            if let parsed = WorkflowLayout.outline(of: detail.graph) {
                outline = parsed
            } else {
                mode = .fields
                outline = nil
            }
        } else {
            outline = nil
        }
        dirty = false
        pending = false
        model.setWorkflowGraphDirty(workflowId, false)
        model.noteWorkflow(detail.summary)
        if conflict {
            editing = nil
            adding = nil
            model.noteWorkflow(workflowId, notice: "This workflow was edited somewhere else")
        }
    }

    private func loadSchedules() async {
        guard let client = model.client else { return }
        var lines: [String: String] = [:]
        for node in graph.nodes where node.kind == .trigger && node.string("trigger") == "schedule" {
            guard let schedule = node.schedule else { continue }
            let zone = schedule.timeZone ?? TimeZone.current.identifier
            if let preview = try? await client.routineSchedule(triggers: [schedule], timeZone: zone) {
                lines[node.id] = preview.summary
            }
        }
        scheduleLines = lines
    }

    private func replace(_ node: WorkflowNode) {
        guard let index = graph.nodes.firstIndex(where: { $0.id == node.id }) else { return }
        guard graph.nodes[index] != node else { return }
        graph.nodes[index] = node
        if mode == .outline, let outline {
            graph = WorkflowLayout.graph(from: outline, keeping: graph)
        }
        if node.kind == .trigger { Task { await loadSchedules() } }
        touch()
    }

    private func add(_ kind: WorkflowNodeKind, route: AddRoute) {
        guard var outline, mode == .outline else { return }
        let node = WorkflowNode.make(kind)
        if kind == .condition {
            guard outline.condition == nil, let after = route.after else { return }
            if after == outline.trigger {
                outline.after = outline.main + outline.after
                outline.main = []
            } else if let index = outline.main.firstIndex(of: after) {
                let rest = Array(outline.main[(index + 1)...])
                outline.main = Array(outline.main[...index])
                outline.after = rest + outline.after
            } else {
                return
            }
            outline.condition = node.id
        } else if let column = route.column, route.after == nil {
            outline[keyPath: column.path].append(node.id)
        } else if let after = route.after {
            if after == outline.trigger {
                outline.main.insert(node.id, at: 0)
            } else if after == outline.condition {
                outline.after.insert(node.id, at: 0)
            } else if let (column, index) = locate(after, outline) {
                outline[keyPath: column.path].insert(node.id, at: index + 1)
            } else {
                return
            }
        } else {
            return
        }
        graph.nodes.append(node)
        self.outline = outline
        rebuild()
    }

    private func duplicate(_ id: String) {
        guard var outline, let node = graph.node(id), node.kind != .trigger, node.kind != .condition,
              let (column, index) = locate(id, outline) else { return }
        var copy = node
        copy.id = UUID().uuidString
        outline[keyPath: column.path].insert(copy.id, at: index + 1)
        graph.nodes.append(copy)
        self.outline = outline
        rebuild()
    }

    private func delete(_ id: String) {
        guard var outline, id != outline.trigger else { return }
        if outline.condition == id {
            outline.main.append(contentsOf: outline.yes + outline.no + outline.after)
            outline.condition = nil
            outline.yes = []
            outline.no = []
            outline.after = []
        } else if let (column, index) = locate(id, outline) {
            outline[keyPath: column.path].remove(at: index)
        } else {
            outline.loose.removeAll { $0 == id }
        }
        graph.nodes.removeAll { $0.id == id }
        self.outline = outline
        rebuild()
    }

    private func move(_ id: String, by delta: Int) {
        guard var outline, let (column, index) = locate(id, outline) else { return }
        var list = outline[keyPath: column.path]
        let next = index + delta
        guard list.indices.contains(next) else { return }
        list.swapAt(index, next)
        outline[keyPath: column.path] = list
        self.outline = outline
        rebuild()
    }

    private func adopt(_ id: String) {
        guard var outline else { return }
        outline.loose.removeAll { $0 == id }
        if graph.node(id)?.kind == .condition, outline.condition == nil {
            outline.condition = id
        } else {
            outline.main.append(id)
        }
        self.outline = outline
        rebuild()
    }

    private func rebuild() {
        guard let outline else { return }
        graph = WorkflowLayout.graph(from: outline, keeping: graph)
        touch()
    }

    private func locate(_ id: String, _ outline: WorkflowOutline) -> (OutlineColumn, Int)? {
        for column in OutlineColumn.allCases {
            if let index = outline[keyPath: column.path].firstIndex(of: id) { return (column, index) }
        }
        return nil
    }

    private func touch() {
        guard mode != .readOnly, !model.deletedWorkflows.contains(workflowId) else { return }
        dirty = true
        model.setWorkflowGraphDirty(workflowId, true)
        model.noteWorkflow(workflowId, notice: nil)
        saveTask?.cancel()
        saveTask = Task {
            try? await Task.sleep(for: .milliseconds(400))
            guard !Task.isCancelled else { return }
            await saveNow()
        }
    }

    private func flush() {
        saveTask?.cancel()
        guard dirty, mode != .readOnly else { return }
        Task { await saveNow() }
    }

    private func saveNow() async {
        if saving {
            pending = true
            return
        }
        saving = true
        defer { saving = false }
        guard let client = model.client else { return }
        repeat {
            pending = false
            let snapshot = graph
            let revision = baseRevision
            do {
                let result = try await client.saveWorkflowGraph(workflowId, baseRevision: revision, graph: snapshot)
                if result.isConflict {
                    apply(result.workflow, conflict: true)
                    await loadSchedules()
                    return
                }
                baseRevision = result.revision ?? result.workflow.revision
                title = result.workflow.title
                webhooks = result.workflow.webhooks
                model.noteWorkflow(result.workflow.summary)
                if graph != snapshot { pending = true }
            } catch {
                self.error = error.localizedDescription
                return
            }
        } while pending
        dirty = false
        model.setWorkflowGraphDirty(workflowId, false)
    }

    private func stopEditing() {
        saveTask?.cancel()
        dirty = false
        pending = false
        editing = nil
        adding = nil
        model.setWorkflowGraphDirty(workflowId, false)
    }

    private func revealWebhook() async -> [WorkflowWebhook] {
        guard let client = model.client else { return webhooks }
        if let detail = try? await client.workflow(workflowId, reveal: true) {
            webhooks = detail.webhooks
        }
        return webhooks
    }

    private func rotateWebhook(_ nodeId: String) async -> String? {
        guard let client = model.client else { return nil }
        do {
            let secret = try await client.rotateWorkflowWebhook(workflowId, nodeId: nodeId)
            if let index = webhooks.firstIndex(where: { $0.nodeId == nodeId }) {
                webhooks[index].secret = secret
            }
            return secret
        } catch {
            self.error = error.localizedDescription
            return nil
        }
    }
}

struct StepRoute: Identifiable, Hashable {
    var id: String
}

struct AddRoute: Identifiable, Hashable {
    var column: OutlineColumn?
    var after: String?
    var id: String { "\(column?.rawValue ?? "")/\(after ?? "")" }

    init(column: OutlineColumn? = nil, after: String? = nil) {
        self.column = column
        self.after = after
    }
}

enum OutlineColumn: String, CaseIterable {
    case main, yes, no, after

    var path: WritableKeyPath<WorkflowOutline, [String]> {
        switch self {
        case .main: \.main
        case .yes: \.yes
        case .no: \.no
        case .after: \.after
        }
    }
}
