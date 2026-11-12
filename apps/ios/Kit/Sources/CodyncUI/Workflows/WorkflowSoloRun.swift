import CodyncKit
import SwiftUI

/// Runs one step outside a full workflow run. The same action the other clients show on the step sheet.
struct WorkflowSoloRun: View {
    let workflowId: String
    let nodeId: String
    let kind: WorkflowNodeKind
    /// An approval a notification already created. Its buttons replace Run until it is answered.
    var pendingStepId: String? = nil
    /// Saves the editor first. A non-nil string means the step must not start.
    var prepare: () async -> String? = { nil }

    @Environment(BotStore.self) private var model
    @State private var running = false
    @State private var stepId: String?
    @State private var approvalId: String?
    @State private var decided = false
    @State private var busy = false
    @State private var error: String?

    private var waitingApproval: String? {
        if decided { return nil }
        return approvalId ?? pendingStepId
    }

    var body: some View {
        if showsRun {
            VStack(alignment: .leading, spacing: 8) {
                if let error {
                    Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger)
                }
                if let approval = waitingApproval {
                    HStack(spacing: 10) {
                        Button(L10n.tr("Approve")) { Task { await answer(approval, approved: true) } }
                            .buttonStyle(.primary)
                        Button(L10n.tr("Reject")) { Task { await answer(approval, approved: false) } }
                            .buttonStyle(.secondary)
                    }
                    .disabled(busy || model.isOffline)
                } else {
                    Button { Task { await toggle() } } label: {
                        Text(L10n.tr(running ? "Stop" : "Run")).frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.primary)
                    .disabled(busy || model.isOffline)
                }
            }
        }
    }

    private var showsRun: Bool {
        switch kind {
        case .agent, .condition, .approval, .output: true
        default: false
        }
    }

    private func answer(_ id: String, approved: Bool) async {
        guard let client = model.client else { return }
        busy = true
        defer { busy = false }
        do {
            try await client.decideWorkflowStep(id, approved: approved)
            decided = true
            approvalId = nil
            running = false
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func toggle() async {
        guard let client = model.client else { return }
        busy = true
        defer { busy = false }
        if running, let stepId {
            do {
                try await client.stopWorkflowStep(stepId)
                running = false
                error = nil
            } catch {
                self.error = error.localizedDescription
            }
            return
        }
        if let message = await prepare() {
            error = message
            return
        }
        do {
            let step = try await client.runWorkflowStep(workflowId, nodeId: nodeId, requestId: UUID().uuidString)
            self.stepId = step.id
            running = step.status == "running"
            if step.status == "approval" {
                approvalId = step.id
                decided = false
            }
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
