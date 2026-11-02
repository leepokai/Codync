import CodyncKit
import SwiftUI

/// One run: each step's status and output, approval in place, retry on a failure.
struct WorkflowRunView: View {
    let runId: String

    @Environment(BotStore.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var run: WorkflowRunDetail?
    @State private var expanded: Set<String> = []
    @State private var expandedInput: Set<String> = []
    @State private var error: String?
    @State private var busy = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                if let run {
                    overview(run)
                    if run.summary.isOpen {
                        Button(L10n.tr("Stop")) { Task { await stop(run) } }
                            .buttonStyle(RunStopStyle())
                            .disabled(busy || run.status == "stopping" || model.isOffline)
                    }
                    if let error {
                        Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger).textSelection(.enabled)
                    }
                    ForEach(orderedSteps(run)) { step in
                        stepCard(step)
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
        .toolbar(.hidden, for: .tabBar)
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

    private func overview(_ run: WorkflowRunDetail) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .center, spacing: 10) {
                Text(subtitle(run))
                    .font(.subheadline)
                    .foregroundStyle(Palette.secondary)
                Spacer(minLength: 8)
                RunMark(status: run.status)
            }
            if let fraction = progressFraction(run) {
                RunProgress(fraction: fraction, status: run.status)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func stepCard(_ step: WorkflowStep) -> some View {
        let open = expanded.contains(step.id)
        let inputOpen = expandedInput.contains(step.id)
        let live = step.status == "running" || step.status == "stopping"
        return VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .center, spacing: 10) {
                Image(systemName: kindSymbol(step.kind))
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(Palette.text)
                    .frame(width: 32, height: 32)
                    .background(Palette.background, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                Text(shownTitle(step))
                    .font(.body.weight(.medium))
                    .foregroundStyle(Palette.text)
                    .lineLimit(2)
                Spacer(minLength: 8)
                RunStatusPill(status: step.status)
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
            if !step.input.isEmpty {
                disclosure(L10n.tr(inputOpen ? "Hide input" : "Show input"), open: inputOpen) {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                        if inputOpen { expandedInput.remove(step.id) } else { expandedInput.insert(step.id) }
                    }
                }
                if inputOpen { quote(step.input) }
            }
            if let output = step.output, !output.isEmpty {
                disclosure(outputLabel(step, open: open), open: open) {
                    withAnimation(Motion.reduced(Motion.layout, reduceMotion)) {
                        if open { expanded.remove(step.id) } else { expanded.insert(step.id) }
                    }
                }
                if open { quote(output) }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(live ? Palette.bubbleUser : Palette.surface, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
    }

    private func disclosure(_ title: String, open: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Text(title)
                    .font(.callout)
                    .foregroundStyle(Palette.secondary)
                    .lineLimit(2)
                Spacer(minLength: 8)
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(Palette.tertiary)
                    .rotationEffect(.degrees(open ? 90 : 0))
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private func quote(_ text: String) -> some View {
        Text(text)
            .font(.callout)
            .foregroundStyle(Palette.text)
            .textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(12)
            .background(Palette.background, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private func outputLabel(_ step: WorkflowStep, open: Bool) -> String {
        let verb = L10n.tr(open ? "Hide output" : "Show output")
        guard !open, let started = step.startedAt, let ended = step.endedAt else { return verb }
        let time = Date(milliseconds: ended).formatted(date: .abbreviated, time: .shortened)
        return "\(verb) · \(time) · \(WorkflowCopy.duration(from: started, to: ended))"
    }

    private func shownTitle(_ step: WorkflowStep) -> String {
        switch step.title {
        case "Agent", "Notification", "Prompt", "Condition", "Approval", "Chat", "Manual", "Schedule", "Webhook", "Note":
            L10n.tr(step.title)
        default:
            step.title
        }
    }

    private func kindSymbol(_ kind: String) -> String {
        switch kind {
        case "trigger": "bolt.fill"
        case "prompt": "text.alignleft"
        case "agent": "person.fill"
        case "condition": "arrow.triangle.branch"
        case "approval": "checkmark.seal"
        case "output": "bell.fill"
        default: "note.text"
        }
    }

    private func progressFraction(_ run: WorkflowRunDetail) -> Double? {
        let steps = orderedSteps(run)
        guard !steps.isEmpty else { return nil }
        let done = steps.filter { ["done", "failed", "stopped", "skipped"].contains($0.status) }.count
        return Double(done) / Double(steps.count)
    }

    private func reload() async {
        guard let client = model.client else { return }
        do {
            let loaded = try await client.workflowRun(runId)
            run = loaded
            if expanded.isEmpty {
                expanded = Set(loaded.steps.compactMap { step in
                    guard let output = step.output, !output.isEmpty else { return nil }
                    return step.id
                })
            }
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

/// A colored mark for the run. Ink comes from the palette, so light and dark stay readable.
private struct RunMark: View {
    let status: String

    var body: some View {
        Image(systemName: symbol)
            .font(.body.weight(.semibold))
            .foregroundStyle(RunTone.ink(status))
            .accessibilityLabel(L10n.tr(WorkflowCopy.status(status) ?? status))
    }

    private var symbol: String {
        switch status {
        case "done": "checkmark.circle.fill"
        case "failed": "xmark.circle.fill"
        case "approval": "exclamationmark.circle.fill"
        case "running", "stopping": "circle.fill"
        default: "circle"
        }
    }
}

/// Status word on a faint wash of the same ink.
private struct RunStatusPill: View {
    let status: String

    var body: some View {
        let ink = RunTone.ink(status)
        HStack(spacing: 4) {
            if let mark = RunTone.mark(status) {
                Image(systemName: mark).font(.caption2.weight(.bold))
            }
            Text(L10n.tr(WorkflowCopy.status(status) ?? status))
                .font(.caption.weight(.medium))
                .lineLimit(1)
        }
        .foregroundStyle(ink)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(ink.opacity(0.14), in: Capsule())
    }
}

/// How far the run has got. The track is the dim ink; the fill is the status color.
private struct RunProgress: View {
    var fraction: Double
    var status: String

    var body: some View {
        GeometryReader { geo in
            Capsule().fill(Palette.accentDim)
            Capsule()
                .fill(RunTone.ink(status))
                .frame(width: max(0, geo.size.width * fraction))
        }
        .frame(height: 4)
        .accessibilityLabel(L10n.tr(WorkflowCopy.status(status) ?? status))
    }
}

private enum RunTone {
    static func ink(_ status: String) -> Color {
        switch status {
        case "done": Palette.added
        case "failed": Palette.danger
        case "approval": Palette.attention
        case "running", "stopping": Palette.accent
        default: Palette.secondary
        }
    }

    static func mark(_ status: String) -> String? {
        switch status {
        case "done": "checkmark.circle.fill"
        case "failed": "xmark.circle.fill"
        case "approval": "exclamationmark.circle.fill"
        default: nil
        }
    }
}

/// Stop, in the danger color. Light mode fills it; dark mode uses the pale danger ink on a wash, which stays readable.
private struct RunStopStyle: ButtonStyle {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        let dark = scheme == .dark
        configuration.label
            .font(.body.weight(.medium))
            .foregroundStyle(dark ? Palette.danger : Color.white)
            .padding(.horizontal, 22)
            .padding(.vertical, 11)
            .background(dark ? Palette.danger.opacity(0.18) : Palette.danger, in: Capsule())
            .opacity(isEnabled ? (configuration.isPressed ? 0.85 : 1) : 0.4)
            .scaleEffect(configuration.isPressed ? Motion.pressScale : 1)
            .animation(Motion.press, value: configuration.isPressed)
            .contentShape(Capsule())
    }
}

/// Recent runs, 20 at a time. The next page asks for 20 more.
struct WorkflowRunsList: View {
    let workflowId: String
    let title: String

    @Environment(BotStore.self) private var model
    @State private var runs: [WorkflowRunSummary] = []
    @State private var hasMore = false
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
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                if hasMore {
                    Button(L10n.tr("Show older runs")) {
                        guard let last = runs.last else { return }
                        Task { await reload(before: (startedAt: last.startedAt, id: last.id)) }
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
        .toolbar(.hidden, for: .tabBar)
        .navigationDestination(item: $opened) { route in
            WorkflowRunView(runId: route.id)
                .environment(model)
        }
        .task { await reload(before: nil) }
        .onChange(of: model.workflowRunReload) { _, _ in Task { await reload(before: nil) } }
    }

    private func reload(before cursor: (startedAt: Int64, id: String)?) async {
        guard let client = model.client else { return }
        do {
            let page = try await client.workflowRuns(workflowId, limit: 20, before: cursor)
            if let cursor {
                let seen = Set(runs.map(\.id))
                runs.append(contentsOf: page.runs.filter { !seen.contains($0.id) && $0.id != cursor.id })
            } else {
                runs = page.runs
            }
            hasMore = page.hasMore
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
