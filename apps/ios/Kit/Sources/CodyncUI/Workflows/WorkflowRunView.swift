import CodyncKit
import SwiftUI

/// One run: each step's status and output, approval in place, retry on a failure.
struct WorkflowRunView: View {
    let runId: String

    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var run: WorkflowRunDetail?
    @State private var expanded: Set<String> = []
    @State private var error: String?
    @State private var busy = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                if let run {
                    Text(subtitle(run)).font(.subheadline).foregroundStyle(Palette.secondary)
                    if run.summary.isOpen {
                        Button(L10n.tr("Stop")) { Task { await stop(run) } }
                            .buttonStyle(.primary)
                            .disabled(busy || run.status == "stopping" || model.isOffline)
                    }
                    if let error {
                        Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
                    }
                    ForEach(orderedSteps(run)) { step in
                        stepBlock(step)
                    }
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
        .navigationTitle(run.map { WorkflowCopy.runTitle($0.number) } ?? "")
        .navigationBarTitleDisplayMode(.large)
        .task(id: model.workflowRunReload) { await reload() }
        .tracked(.screenViewed(.workflowRun))
    }

    private func subtitle(_ run: WorkflowRunDetail) -> String {
        if run.summary.isOpen { return L10n.tr(WorkflowCopy.status(run.status) ?? "Running") }
        guard let ended = run.endedAt, let status = WorkflowCopy.status(run.status) else {
            return L10n.tr(WorkflowCopy.status(run.status) ?? run.status)
        }
        let time = Date(milliseconds: ended).formatted(date: .abbreviated, time: .shortened)
        let duration = WorkflowCopy.duration(from: run.startedAt, to: ended)
        return L10n.tr("\(status) · \(time) · \(duration)")
    }

    private func orderedSteps(_ run: WorkflowRunDetail) -> [WorkflowStep] {
        let order = WorkflowLayout.readingOrder(of: run.graph)
        let ranked = Dictionary(uniqueKeysWithValues: order.enumerated().map { ($0.element, $0.offset) })
        return run.steps.sorted {
            let left = ranked[$0.nodeId] ?? 0
            let right = ranked[$1.nodeId] ?? 0
            if left != right { return left < right }
            return $0.attempt < $1.attempt
        }
    }

    private func stepBlock(_ step: WorkflowStep) -> some View {
        let open = expanded.contains(step.id)
        return VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(step.title).font(.body.weight(.medium)).foregroundStyle(Palette.text).lineLimit(2)
                Spacer(minLength: 8)
                Text(L10n.tr(WorkflowCopy.status(step.status) ?? step.status))
                    .font(.caption.weight(.medium))
                    .foregroundStyle(step.status == "failed" ? Palette.danger : Palette.secondary)
            }
            if step.status == "approval" {
                HStack(spacing: 10) {
                    Button(L10n.tr("Approve")) { Task { await decide(step, approved: true) } }
                        .buttonStyle(.primary)
                    Button(L10n.tr("Reject")) { Task { await decide(step, approved: false) } }
                        .buttonStyle(.secondary)
                }
                .disabled(busy || model.isOffline)
            }
            if step.status == "failed" {
                if let message = step.error, !message.isEmpty {
                    Text(L10n.tr(message)).font(.caption).foregroundStyle(Palette.danger)
                }
                Button(L10n.tr("Retry")) { Task { await retry(step) } }
                    .buttonStyle(.secondary)
                    .disabled(busy || model.isOffline)
            }
            if let output = step.output, !output.isEmpty {
                Button(L10n.tr(open ? "Hide output" : "Show output")) {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                        if open { expanded.remove(step.id) } else { expanded.insert(step.id) }
                    }
                }
                .font(.callout)
                .buttonStyle(.plain)
                .foregroundStyle(Palette.secondary)
                if open {
                    Text(output).font(.callout).foregroundStyle(Palette.text).textSelection(.enabled)
                }
            }
        }
        .padding(.vertical, 8)
    }

    private func reload() async {
        guard let client = model.client else { return }
        do {
            run = try await client.workflowRun(runId)
            error = nil
        } catch {
            if run == nil { self.error = error.localizedDescription }
        }
    }

    private func stop(_ run: WorkflowRunDetail) async {
        guard let client = model.client else { return }
        busy = true
        defer { busy = false }
        do {
            try await client.stopWorkflowRun(run.id)
            await reload()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func decide(_ step: WorkflowStep, approved: Bool) async {
        guard let client = model.client else { return }
        busy = true
        defer { busy = false }
        do {
            try await client.decideWorkflowStep(step.id, approved: approved)
            await reload()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func retry(_ step: WorkflowStep) async {
        guard let client = model.client else { return }
        busy = true
        defer { busy = false }
        do {
            _ = try await client.retryWorkflowStep(step.id)
            await reload()
        } catch {
            self.error = error.localizedDescription
        }
    }
}

/// Recent runs, 20 at a time. The next page asks for 20 more.
struct WorkflowRunsList: View {
    let workflowId: String
    let title: String

    @Environment(BotStore.self) private var model
    @State private var runs: [WorkflowRunSummary] = []
    @State private var hasMore = false
    @State private var limit = 20
    @State private var opened: WorkflowRunRoute?
    @State private var error: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 4) {
                if let error {
                    Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger)
                }
                ForEach(runs) { run in
                    Button { opened = WorkflowRunRoute(id: run.id) } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(WorkflowCopy.runTitle(run.number)).foregroundStyle(Palette.text)
                            HStack(spacing: 6) {
                                if let status = WorkflowCopy.status(run.status) {
                                    Text(L10n.tr(status))
                                }
                                Text(Date(milliseconds: run.startedAt).formatted(date: .abbreviated, time: .shortened))
                                if let ended = run.endedAt {
                                    Text(WorkflowCopy.duration(from: run.startedAt, to: ended))
                                }
                            }
                            .font(.caption)
                            .foregroundStyle(Palette.secondary)
                        }
                        .padding(.vertical, 10)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                }
                if hasMore {
                    Button(L10n.tr("Show older runs")) {
                        limit += 20
                        Task { await reload() }
                    }
                    .buttonStyle(.secondary)
                    .padding(.top, 8)
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Palette.background)
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
        .navigationDestination(item: $opened) { route in
            WorkflowRunView(runId: route.id)
        }
        .task { await reload() }
        .onChange(of: model.workflowRunReload) { _, _ in Task { await reload() } }
    }

    private func reload() async {
        guard let client = model.client else { return }
        do {
            let page = try await client.workflowRuns(workflowId, limit: limit)
            runs = page.runs
            hasMore = page.hasMore
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
