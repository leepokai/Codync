import CodyncKit
import Foundation

extension BotStore {
    /// Replaces the summary list. A failure leaves the list as it was; the screen shows its own error.
    @discardableResult
    func reloadWorkflows() async -> String? {
        guard !retired, hello?.workflows == true, let client else {
            if hello?.workflows != true { workflows = [] }
            return nil
        }
        do {
            let list = try await client.workflows()
            guard !Task.isCancelled, !retired else { return nil }
            workflows = WorkflowSummary.listOrder(list.filter { !deletedWorkflows.contains($0.id) })
            return nil
        } catch is CancellationError {
            return nil
        } catch {
            return error.localizedDescription
        }
    }

    /// Workflows change on the computer: each change refreshes the summaries; `run` and `step` also open runs, `graph`
    /// an open workflow.
    func applyWorkflow(_ event: WorkflowEvent) {
        switch event.change {
        case .deleted:
            workflows = workflows.filter { $0.id != event.workflowId }
            var gone = deletedWorkflows
            gone.insert(event.workflowId)
            deletedWorkflows = gone
        case .enabled:
            Task { await reloadWorkflows() }
        case .run, .step:
            workflowRunReload += 1
            Task { await reloadWorkflows() }
        case .graph:
            workflowReload += 1
            Task { await reloadWorkflows() }
        }
    }
}
