import Foundation

public extension HostClient {
    func workflows() async throws -> [WorkflowSummary] {
        struct Response: Decodable { var workflows: [WorkflowSummary] }
        let response: Response = try await call("workflows")
        return response.workflows
    }

    func workflow(_ id: String, reveal: Bool = false) async throws -> WorkflowDetail {
        struct Body: Encodable { var id: String; var reveal: Bool }
        struct Response: Decodable { var workflow: WorkflowDetail }
        let response: Response = try await call("workflow", Body(id: id, reveal: reveal))
        return response.workflow
    }

    func createWorkflow(title: String, graph: WorkflowGraph? = nil) async throws -> WorkflowDetail {
        struct Body: Encodable { var title: String; var graph: WorkflowGraph? }
        struct Response: Decodable { var workflow: WorkflowDetail }
        let response: Response = try await call("createWorkflow", Body(title: title, graph: graph))
        return response.workflow
    }

    func renameWorkflow(_ id: String, title: String) async throws -> WorkflowSummary {
        struct Body: Encodable { var id: String; var title: String }
        struct Response: Decodable { var workflow: WorkflowSummary }
        let response: Response = try await call("renameWorkflow", Body(id: id, title: title))
        return response.workflow
    }

    func deleteWorkflow(_ id: String) async throws {
        struct Body: Encodable { var id: String }
        let _: Empty = try await call("deleteWorkflow", Body(id: id))
    }

    func setWorkflowEnabled(_ id: String, enabled: Bool) async throws -> WorkflowSummary {
        struct Body: Encodable { var id: String; var enabled: Bool }
        struct Response: Decodable { var workflow: WorkflowSummary }
        let response: Response = try await call("setWorkflowEnabled", Body(id: id, enabled: enabled))
        return response.workflow
    }

    func saveWorkflowGraph(_ id: String, baseRevision: Int64, graph: WorkflowGraph) async throws -> WorkflowSaveResult {
        struct Body: Encodable { var id: String; var baseRevision: Int64; var graph: WorkflowGraph }
        return try await call("saveWorkflowGraph", Body(id: id, baseRevision: baseRevision, graph: graph))
    }

    func workflowVersions(_ id: String) async throws -> [WorkflowVersion] {
        struct Body: Encodable { var id: String }
        struct Response: Decodable { var versions: [WorkflowVersion] }
        let response: Response = try await call("workflowVersions", Body(id: id))
        return response.versions
    }

    func workflowVersion(_ id: String, versionId: String) async throws -> WorkflowGraph {
        struct Body: Encodable { var id: String; var versionId: String }
        struct Response: Decodable { var graph: WorkflowGraph }
        let response: Response = try await call("workflowVersion", Body(id: id, versionId: versionId))
        return response.graph
    }

    func restoreWorkflowVersion(_ id: String, versionId: String) async throws -> WorkflowDetail {
        struct Body: Encodable { var id: String; var versionId: String }
        struct Response: Decodable { var workflow: WorkflowDetail }
        let response: Response = try await call("restoreWorkflowVersion", Body(id: id, versionId: versionId))
        return response.workflow
    }

    func startWorkflowRun(_ id: String, requestId: String, fromNodeId: String? = nil) async throws -> WorkflowRunDetail {
        struct Body: Encodable { var id: String; var requestId: String; var fromNodeId: String? }
        struct Response: Decodable { var run: WorkflowRunDetail }
        let response: Response = try await call("startWorkflowRun", Body(id: id, requestId: requestId, fromNodeId: fromNodeId))
        return response.run
    }

    func stopWorkflowRun(_ runId: String) async throws {
        struct Body: Encodable { var runId: String }
        let _: Empty = try await call("stopWorkflowRun", Body(runId: runId))
    }

    func runWorkflowStep(_ id: String, nodeId: String, requestId: String) async throws -> WorkflowStep {
        struct Body: Encodable { var id: String; var nodeId: String; var requestId: String }
        struct Response: Decodable { var step: WorkflowStep }
        let response: Response = try await call("runWorkflowStep", Body(id: id, nodeId: nodeId, requestId: requestId))
        return response.step
    }

    func stopWorkflowStep(_ stepId: String) async throws {
        struct Body: Encodable { var stepId: String }
        let _: Empty = try await call("stopWorkflowStep", Body(stepId: stepId))
    }

    func retryWorkflowStep(_ stepId: String) async throws -> WorkflowStep {
        struct Body: Encodable { var stepId: String }
        struct Response: Decodable { var step: WorkflowStep }
        let response: Response = try await call("retryWorkflowStep", Body(stepId: stepId))
        return response.step
    }

    func decideWorkflowStep(_ stepId: String, approved: Bool) async throws {
        struct Body: Encodable { var stepId: String; var approved: Bool }
        let _: Empty = try await call("decideWorkflowStep", Body(stepId: stepId, approved: approved))
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

    func rotateWorkflowWebhook(_ id: String, nodeId: String) async throws -> String {
        struct Body: Encodable { var id: String; var nodeId: String }
        struct Response: Decodable { var secret: String }
        let response: Response = try await call("rotateWorkflowWebhook", Body(id: id, nodeId: nodeId))
        return response.secret
    }
}
