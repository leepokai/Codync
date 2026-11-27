import Foundation

/// Workflows on the phone are read only, apart from answering an approval step.
public extension HostClient {
    func workflows() async throws -> [WorkflowSummary] {
        struct Response: Decodable { var workflows: [WorkflowSummary] }
        let response: Response = try await call("workflows")
        return response.workflows
    }

    func workflow(_ id: String) async throws -> WorkflowDetail {
        struct Body: Encodable { var id: String }
        struct Response: Decodable { var workflow: WorkflowDetail }
        let response: Response = try await call("workflow", Body(id: id))
        return response.workflow
    }

    func decideWorkflowStep(_ stepId: String, approved: Bool) async throws {
        struct Body: Encodable { var stepId: String; var approved: Bool }
        let _: Empty = try await call("decideWorkflowStep", Body(stepId: stepId, approved: approved))
    }

    /// One step, for a step run on its own outside any run (an approval a push opened).
    func workflowStep(_ stepId: String) async throws -> WorkflowStep {
        struct Body: Encodable { var stepId: String }
        struct Response: Decodable { var step: WorkflowStep }
        let response: Response = try await call("workflowStep", Body(stepId: stepId))
        return response.step
    }

    func workflowRuns(_ id: String, limit: Int, before: (startedAt: Int64, id: String)? = nil) async throws -> WorkflowRunPage {
        struct Before: Encodable { var startedAt: Int64; var id: String }
        struct Body: Encodable { var id: String; var limit: Int; var before: Before? }
        let cursor = before.map { Before(startedAt: $0.startedAt, id: $0.id) }
        return try await call("workflowRuns", Body(id: id, limit: limit, before: cursor))
    }

    func workflowRun(_ runId: String) async throws -> WorkflowRunDetail {
        struct Body: Encodable { var runId: String }
        struct Response: Decodable { var run: WorkflowRunDetail }
        let response: Response = try await call("workflowRun", Body(runId: runId))
        return response.run
    }
}
