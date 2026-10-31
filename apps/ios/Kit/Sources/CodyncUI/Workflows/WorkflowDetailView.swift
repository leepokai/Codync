import CodyncKit
import SwiftUI

/// A workflow: run, edit, steps, triggers, the latest run, delete, and the saved version.
public struct WorkflowDetailView: View {
    let workflowId: String
    var pendingRunId: String?

    @Environment(BotStore.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var detail: WorkflowDetail?
    @State private var versions: [WorkflowVersion] = []
    @State private var runs: [WorkflowRunSummary] = []
    @State private var scheduleLines: [String: String] = [:]
    @State private var error: String?
    @State private var busy = false
    @State private var confirmDelete = false
    @State private var renaming = false
    @State private var renameText = ""
    @State private var editing: WorkflowEditorRoute?
    @State private var showingRuns: WorkflowRunsRoute?
    @State private var openedRun: WorkflowRunRoute?
    @State private var didOpenPendingRun = false

    public init(workflowId: String, pendingRunId: String? = nil) {
        self.workflowId = workflowId
        self.pendingRunId = pendingRunId
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                if model.deletedWorkflows.contains(workflowId) {
                    Text(L10n.tr("This workflow was deleted"))
                        .font(.body)
                        .foregroundStyle(Palette.secondary)
                } else if let detail {
                    notices
                    controls(detail)
                    steps(detail)
                    triggers(detail)
                    latestRun
                    if !versions.isEmpty { versionList }
                    deleteButton
                    Text(savedLine(detail))
                        .font(.caption)
                        .foregroundStyle(Palette.tertiary)
                } else if let error {
                    Text(L10n.tr(error)).font(.callout).foregroundStyle(Palette.danger)
                } else {
                    Spinner()
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Palette.background)
        .navigationTitle(detail?.title ?? "")
        .navigationBarTitleDisplayMode(.large)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button(L10n.tr("Rename"), systemImage: "pencil") {
                    renameText = detail?.title ?? ""
                    renaming = true
                }
                .disabled(detail == nil || model.deletedWorkflows.contains(workflowId) || model.isOffline)
                .accessibilityLabel(L10n.tr("Rename"))
            }
        }
        .navigationDestination(item: $editing) { _ in
            WorkflowEditorView(workflowId: workflowId)
        }
        .navigationDestination(item: $showingRuns) { _ in
            WorkflowRunsList(workflowId: workflowId, title: detail?.title ?? "")
        }
        .navigationDestination(item: $openedRun) { route in
            WorkflowRunView(runId: route.id)
        }
        .task { await reload() }
        .onChange(of: model.workflowReload) { _, _ in Task { await reload() } }
        .onChange(of: model.workflows) { _, _ in applySummary() }
        .onChange(of: model.deletedWorkflows) { _, gone in
            if gone.contains(workflowId) { editing = nil }
        }
        .codyncDialog("Rename", isPresented: $renaming, field: DialogField("Name", text: $renameText)) {
            [DialogAction("Save") { Task { await rename() } }]
        }
        .codyncDialog("Delete workflow", isPresented: $confirmDelete,
                      message: "This deletes the workflow and its runs.") {
            [DialogAction("Delete workflow", destructive: true) { Task { await remove() } }]
        }
        .tracked(.screenViewed(.workflow))
    }

    @ViewBuilder private var notices: some View {
        if let notice = model.workflowNotices[workflowId], !model.deletedWorkflows.contains(workflowId) {
            Text(L10n.tr(notice)).font(.callout).foregroundStyle(Palette.secondary)
        }
        if let error {
            Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
        }
    }

    private func controls(_ detail: WorkflowDetail) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Toggle(isOn: Binding(get: { detail.enabled }, set: { on in
                self.detail?.enabled = on
                Task { await setEnabled(on) }
            })) {
                Text(L10n.tr("Enabled")).foregroundStyle(Palette.text)
            }
            .toggleStyle(.codync)
            .disabled(busy || model.isOffline)
            HStack(spacing: 10) {
                Button(action: runOrStop) {
                    Text(L10n.tr(detail.running == nil ? "Run now" : "Stop"))
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.primary)
                .disabled(busy || model.isOffline)
                Button(action: { editing = WorkflowEditorRoute(id: workflowId) }) {
                    Text(L10n.tr("Edit")).frame(maxWidth: .infinity)
                }
                .buttonStyle(.secondary)
            }
        }
    }

    private func steps(_ detail: WorkflowDetail) -> some View {
        let order = WorkflowLayout.readingOrder(of: detail.graph)
        return VStack(alignment: .leading, spacing: 8) {
            Text(L10n.tr("Steps")).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.secondary)
            ForEach(order, id: \.self) { id in
                if let node = detail.graph.node(id) {
                    WorkflowNodeLine(node: node, schedule: scheduleLines[id], botName: botName)
                }
            }
        }
    }

    private func triggers(_ detail: WorkflowDetail) -> some View {
        let nodes = detail.graph.nodes.filter { $0.kind == .trigger }
        return VStack(alignment: .leading, spacing: 8) {
            Text(L10n.tr("Triggers")).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.secondary)
            ForEach(nodes, id: \.id) { node in
                WorkflowNodeLine(node: node, schedule: scheduleLines[node.id], botName: botName)
            }
        }
    }

    @ViewBuilder private var latestRun: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.tr("Latest results")).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.secondary)
            if let run = runs.first {
                Button { showingRuns = WorkflowRunsRoute(id: workflowId) } label: { runLine(run) }.buttonStyle(.plain)
            } else if let last = detail?.lastRun {
                Button { showingRuns = WorkflowRunsRoute(id: workflowId) } label: {
                    Text(WorkflowCopy.runTitle(last.number)).foregroundStyle(Palette.text).frame(maxWidth: .infinity, alignment: .leading)
                }
                .buttonStyle(.plain)
            }
        }
    }

    private var versionList: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(L10n.tr("Versions")).font(.subheadline.weight(.semibold)).foregroundStyle(Palette.secondary)
            ForEach(versions) { version in
                HStack {
                    Text(versionLine(version)).font(.callout).foregroundStyle(Palette.text)
                    Spacer(minLength: 8)
                    Button(L10n.tr("Make current")) { Task { await restore(version) } }
                        .font(.callout)
                        .buttonStyle(.plain)
                        .foregroundStyle(Palette.secondary)
                        .disabled(busy || model.isOffline)
                }
            }
        }
    }

    private var deleteButton: some View {
        Button(L10n.tr("Delete workflow")) { confirmDelete = true }
            .font(.body.weight(.medium))
            .foregroundStyle(Palette.danger)
            .disabled(busy || model.isOffline)
    }

    private func runLine(_ run: WorkflowRunSummary) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(WorkflowCopy.runTitle(run.number)).foregroundStyle(Palette.text)
            if let status = WorkflowCopy.status(run.status) {
                Text(L10n.tr(status)).font(.caption).foregroundStyle(Palette.secondary)
            }
        }
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func savedLine(_ detail: WorkflowDetail) -> String {
        let time = Date(milliseconds: detail.updatedAt).formatted(date: .abbreviated, time: .shortened)
        return L10n.tr("Version \(detail.revision) · Saved \(time)")
    }

    private func versionLine(_ version: WorkflowVersion) -> String {
        let time = Date(milliseconds: version.createdAt).formatted(date: .abbreviated, time: .shortened)
        switch version.kind {
        case "restore": return L10n.tr("Before this restore · \(time)")
        case "run": return L10n.tr("When a run started · \(time)")
        default: return time
        }
    }

    private func botName(_ id: String) -> String? { model.bots[id]?.name }

    private func applySummary() {
        guard var detail, let summary = model.workflows.first(where: { $0.id == workflowId }),
              summary.revision >= detail.revision else { return }
        detail.enabled = summary.enabled
        detail.running = summary.running
        detail.lastRun = summary.lastRun
        detail.title = summary.title
        detail.updatedAt = summary.updatedAt
        detail.revision = summary.revision
        self.detail = detail
    }

    private func reload() async {
        guard let client = model.client, !model.deletedWorkflows.contains(workflowId) else { return }
        do {
            let loaded = try await client.workflow(workflowId)
            detail = loaded
            error = nil
            await loadScheduleLines(loaded)
            let page = try await client.workflowRuns(workflowId, limit: 20)
            runs = page.runs
            versions = (try? await client.workflowVersions(workflowId)) ?? []
            if let pendingRunId, !didOpenPendingRun {
                didOpenPendingRun = true
                openedRun = WorkflowRunRoute(id: pendingRunId)
            }
        } catch {
            if detail == nil { self.error = error.localizedDescription }
        }
    }

    private func loadScheduleLines(_ detail: WorkflowDetail) async {
        guard let client = model.client else { return }
        var lines: [String: String] = [:]
        for node in detail.graph.nodes where node.kind == .trigger && node.string("trigger") == "schedule" {
            guard let schedule = node.schedule else { continue }
            if let preview = try? await client.routineSchedule(triggers: [schedule], timeZone: schedule.timeZone ?? TimeZone.current.identifier) {
                lines[node.id] = preview.summary
            }
        }
        scheduleLines = lines
    }

    private func setEnabled(_ on: Bool) async {
        guard let client = model.client else { return }
        busy = true
        defer { busy = false }
        do {
            let summary = try await client.setWorkflowEnabled(workflowId, enabled: on)
            model.noteWorkflow(summary)
            detail?.enabled = on
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func runOrStop() {
        guard let client = model.client, let detail else { return }
        busy = true
        Task {
            defer { busy = false }
            do {
                if detail.running != nil {
                    let open = runs.first { $0.isOpen }
                    if let open { try await client.stopWorkflowRun(open.id) }
                } else {
                    let run = try await client.startWorkflowRun(workflowId, requestId: UUID().uuidString)
                    openedRun = WorkflowRunRoute(id: run.id)
                }
                error = nil
                await reload()
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    private func rename() async {
        let title = renameText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty, let client = model.client else { return }
        do {
            let summary = try await client.renameWorkflow(workflowId, title: title)
            model.noteWorkflow(summary)
            detail?.title = summary.title
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func restore(_ version: WorkflowVersion) async {
        guard let client = model.client else { return }
        busy = true
        defer { busy = false }
        do {
            let restored = try await client.restoreWorkflowVersion(workflowId, versionId: version.id)
            detail = restored
            model.noteWorkflow(restored.summary)
            await reload()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func remove() async {
        guard let client = model.client else { return }
        do {
            try await client.deleteWorkflow(workflowId)
            PinnedWorkflows.remove(workflowId, computer: model.computer.id)
            model.applyWorkflow(WorkflowEvent(change: .deleted, workflowId: workflowId, runId: nil, stepId: nil, enabled: nil))
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
    }
}

struct WorkflowRunRoute: Identifiable, Hashable {
    var id: String
}

struct WorkflowRunsRoute: Identifiable, Hashable {
    var id: String
}

struct WorkflowNodeLine: View {
    let node: WorkflowNode
    var schedule: String?
    var caption: String?
    var botName: (String) -> String?

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: WorkflowCopy.symbol(node.kind))
                .font(.body.weight(.semibold))
                .foregroundStyle(Palette.secondary)
                .frame(width: 28, height: 28)
            VStack(alignment: .leading, spacing: 2) {
                Text(L10n.tr(WorkflowCopy.title(node, botName: botName)))
                    .font(.body.weight(.medium))
                    .foregroundStyle(Palette.text)
                    .lineLimit(1)
                Text(L10n.tr(WorkflowCopy.summary(node, botName: botName, schedule: schedule)))
                    .font(.caption)
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(2)
                if let caption {
                    Text(L10n.tr(caption)).font(.caption).foregroundStyle(Palette.secondary)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
