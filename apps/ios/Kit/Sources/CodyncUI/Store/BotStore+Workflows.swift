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

    func noteWorkflow(_ summary: WorkflowSummary) {
        var next = workflows.filter { $0.id != summary.id }
        next.append(summary)
        workflows = WorkflowSummary.listOrder(next)
    }

    func setWorkflowGraphDirty(_ id: String, _ dirty: Bool) {
        var next = dirtyWorkflowGraphs
        if dirty { next.insert(id) } else { next.remove(id) }
        dirtyWorkflowGraphs = next
    }

    func noteWorkflow(_ id: String, notice: String?) {
        var next = workflowNotices
        if let notice { next[id] = notice } else { next.removeValue(forKey: id) }
        workflowNotices = next
    }

    /// `enabled`, `run` and `step` refresh summaries and open runs. A dirty graph is left for the next save.
    func applyWorkflow(_ event: WorkflowEvent) {
        switch event.change {
        case .deleted:
            workflows = workflows.filter { $0.id != event.workflowId }
            var gone = deletedWorkflows
            gone.insert(event.workflowId)
            deletedWorkflows = gone
            var dirty = dirtyWorkflowGraphs
            dirty.remove(event.workflowId)
            dirtyWorkflowGraphs = dirty
            noteWorkflow(event.workflowId, notice: "This workflow was deleted")
        case .enabled:
            if let on = event.enabled, let index = workflows.firstIndex(where: { $0.id == event.workflowId }) {
                var next = workflows
                next[index].enabled = on
                workflows = next
            }
            Task { await reloadWorkflows() }
        case .run, .step:
            workflowRunReload += 1
            Task { await reloadWorkflows() }
        case .graph:
            if !dirtyWorkflowGraphs.contains(event.workflowId) { workflowReload += 1 }
            Task { await reloadWorkflows() }
        }
    }
}
