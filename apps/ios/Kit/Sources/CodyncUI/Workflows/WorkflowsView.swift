import CodyncKit
import SwiftUI

/// One computer's workflows. Open runs come first; the rest follow `updatedAt`.
public struct WorkflowsView: View {
    var pendingWorkflowId: String?
    var pendingRunId: String?
    var pendingToken: UUID?
    var onPendingHandled: () -> Void

    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var opened: OpenedWorkflow?
    @State private var editing: WorkflowEditorRoute?
    @State private var error: String?
    @State private var loaded = false
    @State private var drafting = false
    @State private var pinRevision = 0

    public init(pendingWorkflowId: String? = nil, pendingRunId: String? = nil, pendingToken: UUID? = nil, onPendingHandled: @escaping () -> Void = {}) {
        self.pendingWorkflowId = pendingWorkflowId
        self.pendingRunId = pendingRunId
        self.pendingToken = pendingToken
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
                            Button(L10n.tr("New workflow")) { drafting = true }
                                .buttonStyle(.primary)
                                .disabled(model.isOffline)
                        }
                    }
                    .padding(.top, 8)
                } else {
                    CardSection(footer: "Hold a workflow to pin it on the bot list.") {
                        ForEach(model.workflows) { workflow in
                            row(workflow)
                        }
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
                Button(L10n.tr("New workflow"), systemImage: "plus") { drafting = true }
                    .disabled(model.isOffline)
                    .accessibilityLabel(L10n.tr("New workflow"))
            }
        }
        .navigationDestination(item: $opened) { item in
            WorkflowDetailView(workflowId: item.id, pendingRunId: item.runId)
                .environment(model)
        }
        .navigationDestination(item: $editing) { route in
            WorkflowEditorView(workflowId: route.id)
                .environment(model)
        }
        .task(id: pendingToken) {
            let pending = pendingWorkflowId
            let run = pendingRunId
            let token = pendingToken
            error = await model.reloadWorkflows()
            loaded = true
            // Coming back to the list restarts this task. A claimed link must not push the detail again.
            if let pending, let token, PendingWorkflowOpen.claim(token) {
                opened = OpenedWorkflow(id: pending, runId: run)
                onPendingHandled()
            }
        }
        .codyncSheet(isPresented: $drafting) {
            WorkflowNewSheet { title, graph in
                guard let client = model.client else { return "Offline" }
                do {
                    let created = try await client.createWorkflow(title: title, graph: graph)
                    model.noteWorkflow(created.summary)
                    editing = WorkflowEditorRoute(id: created.id)
                    return nil
                } catch {
                    return error.localizedDescription
                }
            }
        }
    }

    private func row(_ workflow: WorkflowSummary) -> some View {
        Button {
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { opened = OpenedWorkflow(id: workflow.id, runId: nil) }
        } label: {
            WorkflowSummaryLine(workflow: workflow, onCard: true)
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

}

struct OpenedWorkflow: Identifiable, Hashable {
    var id: String
    var runId: String?
}

/// One deep link opens its workflow once, even if the list is created again on the way back.
@MainActor
enum PendingWorkflowOpen {
    private static var claimed: Set<UUID> = []

    static func claim(_ token: UUID) -> Bool { claimed.insert(token).inserted }
}

struct WorkflowEditorRoute: Identifiable, Hashable {
    var id: String
}

public struct WorkflowSummaryLine: View {
    let workflow: WorkflowSummary
    /// The workflows list draws this on a filled card, so the icon sits on the page color.
    var onCard: Bool = false

    public init(workflow: WorkflowSummary, onCard: Bool = false) {
        self.workflow = workflow
        self.onCard = onCard
    }

    public var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "flowchart")
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(Palette.text)
                .frame(width: 32, height: 32)
                .background(onCard ? Palette.background : Palette.surface, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(workflow.title).font(.body.weight(.medium)).foregroundStyle(Palette.text).lineLimit(1)
                WorkflowIdLine(id: workflow.id)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(subtitleColor)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            Image(systemName: "chevron.right")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.tertiary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
    }

    private var subtitle: String {
        guard let status = WorkflowCopy.status(workflow.running ?? workflow.lastRun?.status) else {
            return L10n.tr("Never run")
        }
        let label = L10n.tr(status)
        guard workflow.running == nil, let ended = workflow.lastRun?.endedAt else { return label }
        let date = Date(milliseconds: ended).formatted(date: .abbreviated, time: .omitted)
        return "\(label) · \(date)"
    }

    private var subtitleColor: Color {
        switch workflow.running ?? workflow.lastRun?.status {
        case "failed": Palette.danger
        case "approval": Palette.attention
        default: Palette.secondary
        }
    }
}

/// The workflow's own id, so two workflows with the same name stay distinct.
struct WorkflowIdLine: View {
    let id: String
    var selectable = false

    var body: some View {
        let line = Text(String(id.prefix(8)))
            .font(.caption.monospaced())
            .foregroundStyle(Palette.tertiary)
            .lineLimit(1)
            .truncationMode(.middle)
            .frame(maxWidth: .infinity, alignment: .leading)
        if selectable {
            line.textSelection(.enabled)
        } else {
            line
        }
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
