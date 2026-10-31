import CodyncKit
import SwiftUI

/// One computer's workflows. Open runs come first; the rest follow `updatedAt`.
public struct WorkflowsView: View {
    var pendingWorkflowId: String?
    var pendingRunId: String?
    var onPendingHandled: () -> Void

    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var opened: OpenedWorkflow?
    @State private var editing: WorkflowEditorRoute?
    @State private var error: String?
    @State private var loaded = false
    @State private var creating = false
    @State private var pinRevision = 0

    public init(pendingWorkflowId: String? = nil, pendingRunId: String? = nil, onPendingHandled: @escaping () -> Void = {}) {
        self.pendingWorkflowId = pendingWorkflowId
        self.pendingRunId = pendingRunId
        self.onPendingHandled = onPendingHandled
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let error {
                    Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
                }
                if model.workflows.isEmpty {
                    VStack(alignment: .leading, spacing: 14) {
                        Text(L10n.tr(loaded ? "No workflows yet" : "Loading"))
                            .font(.callout)
                            .foregroundStyle(Palette.secondary)
                        if loaded {
                            Button(L10n.tr("New workflow"), action: create)
                                .buttonStyle(.primary)
                                .disabled(creating || model.isOffline)
                        }
                    }
                    .padding(.top, 8)
                } else {
                    ForEach(model.workflows) { workflow in
                        row(workflow)
                    }
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Palette.background)
        .navigationTitle(L10n.tr("Workflows"))
        .navigationBarTitleDisplayMode(.large)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button(L10n.tr("New workflow"), systemImage: "plus", action: create)
                    .disabled(creating || model.isOffline)
                    .accessibilityLabel(L10n.tr("New workflow"))
            }
        }
        .navigationDestination(item: $opened) { item in
            WorkflowDetailView(workflowId: item.id, pendingRunId: item.runId)
        }
        .navigationDestination(item: $editing) { route in
            WorkflowEditorView(workflowId: route.id)
        }
        .task {
            error = await model.reloadWorkflows()
            loaded = true
            if let pendingWorkflowId {
                opened = OpenedWorkflow(id: pendingWorkflowId, runId: pendingRunId)
                onPendingHandled()
            }
        }
        .tracked(.screenViewed(.workflows))
    }

    private func row(_ workflow: WorkflowSummary) -> some View {
        Button {
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { opened = OpenedWorkflow(id: workflow.id, runId: nil) }
        } label: {
            WorkflowSummaryLine(workflow: workflow)
        }
        .buttonStyle(.plain)
        .contextActions { [pinItem(workflow)] }
    }

    private func pinItem(_ workflow: WorkflowSummary) -> MenuItem {
        let _ = pinRevision
        let pinned = PinnedWorkflows.contains(workflow.id, computer: model.computer.id)
        return MenuItem(pinned ? "Unpin" : "Pin", icon: pinned ? "pin.slash" : "pin") {
            PinnedWorkflows.toggle(workflow.id, computer: model.computer.id)
            pinRevision += 1
        }
    }

    private func create() {
        guard !creating, let client = model.client else { return }
        creating = true
        error = nil
        Task {
            defer { creating = false }
            do {
                let created = try await client.createWorkflow(title: "Workflow", graph: .manualTrigger())
                model.noteWorkflow(created.summary)
                editing = WorkflowEditorRoute(id: created.id)
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

struct OpenedWorkflow: Identifiable, Hashable {
    var id: String
    var runId: String?
}

struct WorkflowEditorRoute: Identifiable, Hashable {
    var id: String
}

public struct WorkflowSummaryLine: View {
    let workflow: WorkflowSummary

    public init(workflow: WorkflowSummary) { self.workflow = workflow }

    public var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(workflow.title).font(.body.weight(.semibold)).foregroundStyle(Palette.text).lineLimit(1)
            if let status = WorkflowCopy.status(workflow.running ?? workflow.lastRun?.status) {
                Text(L10n.tr(status))
                    .font(.subheadline)
                    .foregroundStyle(status == "Failed" ? Palette.danger : Palette.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
    }
}

/// Pins live on this phone only, under `pinnedWorkflows.{computerId}`, at most 20.
public enum PinnedWorkflows {
    public static let limit = 20

    public static func key(_ computer: ComputerID) -> String { "pinnedWorkflows.\(computer)" }

    public static func ids(_ computer: ComputerID, defaults: UserDefaults = .standard) -> [String] {
        defaults.stringArray(forKey: key(computer)) ?? []
    }

    public static func contains(_ id: String, computer: ComputerID, defaults: UserDefaults = .standard) -> Bool {
        ids(computer, defaults: defaults).contains(id)
    }

    public static func toggle(_ id: String, computer: ComputerID, defaults: UserDefaults = .standard) {
        var next = ids(computer, defaults: defaults).filter { $0 != id }
        if !contains(id, computer: computer, defaults: defaults) {
            next.insert(id, at: 0)
            if next.count > limit { next = Array(next.prefix(limit)) }
        }
        defaults.set(next, forKey: key(computer))
    }

    public static func remove(_ id: String, computer: ComputerID, defaults: UserDefaults = .standard) {
        let next = ids(computer, defaults: defaults).filter { $0 != id }
        defaults.set(next, forKey: key(computer))
    }
}
