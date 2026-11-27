import CodyncKit
import SwiftUI

/// A workflow, read only: its steps, triggers and runs. Workflows are made and changed on the computer.
public struct WorkflowDetailView: View {
    let workflowId: String
    var pendingRunId: String?
    /// A step run on its own, outside any run: its push opens it alone.
    var pendingStepId: String?

    @Environment(BotStore.self) private var model
    @State private var detail: WorkflowDetail?
    @State private var runs: [WorkflowRunSummary] = []
    @State private var scheduleLines: [String: String] = [:]
    @State private var error: String?
    @State private var showingRuns: WorkflowRunsRoute?
    @State private var openedRun: WorkflowRunRoute?
    @State private var didOpenPendingRun = false
    @State private var showingDisplay = false
    @AppStorage("workflowStepDisplay") private var displayRaw = WorkflowStepDisplay.labels.rawValue
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var display: WorkflowStepDisplay { WorkflowStepDisplay(rawValue: displayRaw) ?? .labels }

    public init(workflowId: String, pendingRunId: String? = nil, pendingStepId: String? = nil) {
        self.workflowId = workflowId
        self.pendingRunId = pendingRunId
        self.pendingStepId = pendingStepId
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                if model.deletedWorkflows.contains(workflowId) {
                    Text(L10n.tr("This workflow was deleted"))
                        .font(.body)
                        .foregroundStyle(Palette.secondary)
                } else if let detail {
                    if let error {
                        Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
                    }
                    steps(detail)
                    triggers(detail)
                    runHistory
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
        .navigationSubtitle(detail.map(stepCaption) ?? "")
        .navigationBarTitleDisplayMode(.large)
        .toolbar(.hidden, for: .tabBar)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    showingDisplay = true
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: display.symbol)
                        Text(L10n.tr("Mode"))
                    }
                }
                .accessibilityLabel(L10n.tr("Mode"))
            }
        }
        .navigationDestination(item: $showingRuns) { _ in
            WorkflowRunsList(workflowId: workflowId, title: detail?.title ?? "")
                .environment(model)
        }
        .navigationDestination(item: $openedRun) { route in
            WorkflowRunView(target: route.target)
                .environment(model)
        }
        .task { await reload() }
        .onChange(of: model.workflowReload) { _, _ in Task { await reload() } }
        .onChange(of: model.workflows) { _, _ in applySummary() }
        .codyncSheet(isPresented: $showingDisplay) {
            WorkflowDisplaySheet(display: display) { mode in
                withAnimation(Motion.reduced(Motion.layout, reduceMotion)) { displayRaw = mode.rawValue }
            }
        }
        .tracked(.screenViewed(.workflow))
    }

    @ViewBuilder private func steps(_ detail: WorkflowDetail) -> some View {
        let nodes = stepNodes(detail)
        let numbers = detail.graph.stepNumbers()
        if display == .lines {
            VStack(alignment: .leading, spacing: 10) {
                Text(L10n.tr("Steps"))
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Palette.text)
                    .accessibilityAddTraits(.isHeader)
                    .padding(.leading, 2)
                WorkflowStepFlow(graph: detail.graph, schedules: scheduleLines, botName: botName)
                if let footer = stepsFooter(detail) {
                    Text(L10n.tr(footer))
                        .font(.caption)
                        .foregroundStyle(Palette.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, 2)
                }
            }
        } else {
            CardSection("Steps", footer: stepsFooter(detail)) {
                ForEach(nodes, id: \.id) { node in
                    WorkflowNodeLine(node: node, schedule: scheduleLines[node.id], number: numbers[node.id],
                                     parentNumbers: detail.graph.parents(of: node.id).compactMap { numbers[$0] }, botName: botName)
                }
            }
        }
    }

    @ViewBuilder private func triggers(_ detail: WorkflowDetail) -> some View {
        let nodes = detail.graph.nodes.filter { $0.kind == .trigger }
        let numbers = detail.graph.stepNumbers()
        if !nodes.isEmpty {
            CardSection("Triggers") {
                ForEach(nodes, id: \.id) { node in
                    WorkflowNodeLine(node: node, schedule: scheduleLines[node.id],
                                     parentNumbers: detail.graph.parents(of: node.id).compactMap { numbers[$0] }, botName: botName)
                }
            }
        }
    }

    @ViewBuilder private var runHistory: some View {
        if !runs.isEmpty {
            let shown = Array(runs.prefix(3))
            VStack(alignment: .leading, spacing: 10) {
                Text(L10n.tr("Run history"))
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Palette.text)
                    .accessibilityAddTraits(.isHeader)
                    .padding(.leading, 2)
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(shown.enumerated()), id: \.element.id) { index, run in
                        runButton(run)
                        if index < shown.count - 1 || runs.count > shown.count {
                            Hairline().padding(.horizontal, 16)
                        }
                    }
                    if runs.count > shown.count {
                        Button { showingRuns = WorkflowRunsRoute(id: workflowId) } label: {
                            Text(L10n.tr("Show older runs"))
                                .foregroundStyle(Palette.text)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, 16)
                                .padding(.vertical, 14)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            }
        }
    }

    /// The whole row opens the run. The card's padding sits inside the button, so the gray area is part of the tap.
    private func runButton(_ run: WorkflowRunSummary) -> some View {
        Button { openedRun = WorkflowRunRoute(.run(run.id)) } label: {
            runLine(run)
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func runLine(_ run: WorkflowRunSummary) -> some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(WorkflowCopy.runTitle(run.number)).foregroundStyle(Palette.text)
                Text(runSubtitle(run)).font(.caption).foregroundStyle(Palette.secondary).lineLimit(1)
            }
            Spacer(minLength: 8)
            Image(systemName: "chevron.right")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.tertiary)
                .accessibilityHidden(true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func stepNodes(_ detail: WorkflowDetail) -> [WorkflowNode] {
        WorkflowLayout.readingOrder(of: detail.graph).compactMap { detail.graph.node($0) }
            .filter { $0.kind != .trigger && !$0.kind.isAnnotation }
    }

    private func stepCaption(_ detail: WorkflowDetail) -> String {
        let id = String(workflowId.prefix(8))
        let count = stepNodes(detail).count
        guard count > 0 else { return id }
        let steps = L10n.tr("{count} steps").replacingOccurrences(of: "{count}", with: String(count))
        return "\(steps) · \(id)"
    }

    private func stepsFooter(_ detail: WorkflowDetail) -> String? {
        WorkflowLayout.outline(of: detail.graph) == nil ? "The steps show from left to right, as on the canvas." : nil
    }

    private func runSubtitle(_ run: WorkflowRunSummary) -> String {
        var parts: [String] = []
        if let status = WorkflowCopy.status(run.status) { parts.append(L10n.tr(status)) }
        if run.startedAt > 0 {
            parts.append(Date(milliseconds: run.startedAt).formatted(date: .abbreviated, time: .omitted))
        }
        if let ended = run.endedAt {
            parts.append(WorkflowCopy.duration(from: run.startedAt, to: ended))
        }
        return parts.joined(separator: " · ")
    }

    private func savedLine(_ detail: WorkflowDetail) -> String {
        let time = Date(milliseconds: detail.updatedAt).formatted(date: .abbreviated, time: .shortened)
        return L10n.tr("Version \(detail.revision) · Saved \(time)")
    }

    private func botName(_ id: String) -> String? { model.bots[id]?.name }

    private func applySummary() {
        guard var detail, let summary = model.workflows.first(where: { $0.id == workflowId }),
              summary.revision >= detail.revision else { return }
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
            // The run or step a notification named opens before the run history, which it doesn't need.
            if !didOpenPendingRun, let pending = pendingRunId.map(WorkflowRunView.Target.run) ?? pendingStepId.map(WorkflowRunView.Target.step) {
                didOpenPendingRun = true
                openedRun = WorkflowRunRoute(pending)
            }
            await loadScheduleLines(loaded)
            let page = try await client.workflowRuns(workflowId, limit: 20)
            runs = page.runs
        } catch {
            if detail == nil { self.error = error.localizedDescription }
        }
    }

    private func loadScheduleLines(_ detail: WorkflowDetail) async {
        guard let client = model.client else { return }
        var lines: [String: String] = [:]
        for node in detail.graph.nodes where node.kind == .trigger && node.string("trigger") == "schedule" {
            guard let schedule = node.schedule else { continue }
            let zone = schedule.timeZone ?? TimeZone.current.identifier
            do {
                let preview = try await client.routineSchedule(triggers: [schedule], timeZone: zone)
                lines[node.id] = preview.draft.spoken
            } catch is CancellationError {
                return
            } catch {
                if self.error == nil { self.error = error.localizedDescription }
            }
        }
        scheduleLines = lines
    }

}

struct WorkflowRunRoute: Identifiable, Hashable {
    var target: WorkflowRunView.Target
    var id: WorkflowRunView.Target { target }

    init(_ target: WorkflowRunView.Target) { self.target = target }
}

struct WorkflowRunsRoute: Identifiable, Hashable {
    var id: String
}

struct WorkflowNodeLine: View {
    let node: WorkflowNode
    var schedule: String?
    /// This step's place in the list. `parentNumbers` are the steps that feed it, as `From step {number}`.
    var number: Int?
    var parentNumbers: [Int] = []
    var botName: (String) -> String?

    var body: some View {
        let heading = WorkflowCopy.heading(node, botName: botName)
        let summary = WorkflowCopy.summary(node, botName: botName, schedule: schedule)
        let detail = L10n.tr(summary)
        let showsDetail = !summary.isEmpty && detail != heading
        HStack(alignment: .center, spacing: 10) {
            if let number {
                Text("\(number)")
                    .font(.body.monospaced().weight(.semibold))
                    .foregroundStyle(Palette.text)
                    .frame(minWidth: 16, alignment: .trailing)
            }
            Image(systemName: WorkflowCopy.symbol(node.kind))
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(Palette.text)
                .frame(width: 32, height: 32)
                .background(Palette.surface, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                Text(heading)
                    .font(showsDetail ? .caption : .body.weight(.medium))
                    .foregroundStyle(showsDetail ? Palette.secondary : Palette.text)
                    .lineLimit(1)
                if showsDetail {
                    Text(detail)
                        .font(.body)
                        .foregroundStyle(Palette.text)
                        .lineLimit(2)
                }
                if !parentNumbers.isEmpty {
                    WorkflowSourceLine(numbers: parentNumbers)
                }
            }
        }
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
