import CodyncKit
import SwiftUI

/// One computer's workflows. Open runs come first; the rest follow `updatedAt`.
public struct WorkflowsView: View {
    var pendingWorkflowId: String?
    var pendingRunId: String?
    var pendingStepId: String?
    var pendingToken: UUID?
    var onPendingHandled: () -> Void
    /// Search: only the workflows whose title holds it or whose id starts with it.
    var query: String
    /// The workflow pushed over the list, kept by the tab so the list built again (Search opening or closing) keeps it.
    @Binding var opened: OpenedWorkflow?

    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var error: String?
    @State private var loaded = false
    @State private var pinRevision = 0

    public init(opened: Binding<OpenedWorkflow?>, pendingWorkflowId: String? = nil, pendingRunId: String? = nil, pendingStepId: String? = nil,
                pendingToken: UUID? = nil,
                query: String = "", onPendingHandled: @escaping () -> Void = {}) {
        _opened = opened
        self.query = query
        self.pendingWorkflowId = pendingWorkflowId
        self.pendingRunId = pendingRunId
        self.pendingStepId = pendingStepId
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
                    Text(L10n.tr(loaded ? "No workflows yet" : "Loading"))
                        .font(.callout)
                        .foregroundStyle(Palette.secondary)
                        .padding(.top, 8)
                } else if shown.isEmpty {
                    Text(L10n.tr("No results"))
                        .font(.callout)
                        .foregroundStyle(Palette.secondary)
                        .padding(.top, 8)
                } else {
                    CardSection(footer: "Hold a workflow to pin it on the bot list.") {
                        ForEach(shown) { workflow in
                            row(workflow)
                        }
                    }
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Palette.background)
        // The Bots tab's top edge: the list fades softly under the bar.
        .scrollEdgeEffectStyle(.soft, for: .top)
        .navigationTitle(L10n.tr("Workflows"))
        .navigationDestination(item: $opened) { item in
            WorkflowDetailView(workflowId: item.id, pendingRunId: item.runId, pendingStepId: item.stepId)
                .environment(model)
        }
        .task(id: pendingToken) {
            let pending = pendingWorkflowId
            let run = pendingRunId
            let step = pendingStepId
            let token = pendingToken
            error = await model.reloadWorkflows()
            loaded = true
            // Coming back to the list restarts this task. A claimed link must not push the detail again.
            if let pending, let token, PendingWorkflowOpen.claim(token) {
                opened = OpenedWorkflow(id: pending, runId: run, stepId: step)
                onPendingHandled()
            }
        }
    }

    private var shown: [WorkflowSummary] { WorkflowSummary.matching(model.workflows, query) }

    private func row(_ workflow: WorkflowSummary) -> some View {
        Button {
            withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { opened = OpenedWorkflow(id: workflow.id) }
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

public struct OpenedWorkflow: Identifiable, Hashable, Sendable {
    public var id: String
    public var runId: String?
    /// A step run on its own, outside any run.
    public var stepId: String?
}

/// One deep link opens its workflow once, even if the list is created again on the way back.
@MainActor
enum PendingWorkflowOpen {
    private static var claimed: Set<UUID> = []

    static func claim(_ token: UUID) -> Bool { claimed.insert(token).inserted }
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
            Image("workflow")
                .resizable()
                .frame(width: 18, height: 18)
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

    var body: some View {
        Text(String(id.prefix(8)))
            .font(.caption.monospaced())
            .foregroundStyle(Palette.tertiary)
            .lineLimit(1)
            .truncationMode(.middle)
            .frame(maxWidth: .infinity, alignment: .leading)
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
