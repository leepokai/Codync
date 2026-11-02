import CodyncKit
import SwiftUI

/// Runs one step outside a full workflow run. The same action the other clients show on the step sheet.
struct WorkflowSoloRun: View {
    let workflowId: String
    let nodeId: String
    let kind: WorkflowNodeKind
    /// Saves the editor first. A non-nil string means the step must not start.
    var prepare: () async -> String? = { nil }

    @Environment(BotStore.self) private var model
    @State private var running = false
    @State private var stepId: String?
    @State private var error: String?

    var body: some View {
        if showsRun {
            VStack(alignment: .leading, spacing: 8) {
                if let error {
                    Text(L10n.tr(error)).font(.caption).foregroundStyle(Palette.danger)
                }
                Button { Task { await toggle() } } label: {
                    Text(L10n.tr(running ? "Stop" : "Run")).frame(maxWidth: .infinity)
                }
                .buttonStyle(.primary)
            }
        }
    }

    private var showsRun: Bool {
        switch kind {
        case .agent, .condition, .approval, .output: true
        default: false
        }
    }

    private func toggle() async {
        guard let client = model.client else { return }
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
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
