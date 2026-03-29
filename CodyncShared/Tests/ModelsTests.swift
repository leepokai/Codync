import XCTest
@testable import CodyncShared

final class ModelsTests: XCTestCase {
    func testSessionStatusColors() {
        XCTAssertTrue(SessionStatus.working.isActive)
        XCTAssertTrue(SessionStatus.needsInput.isActive)
        XCTAssertFalse(SessionStatus.completed.isActive)
    }

    func testTaskItemTruncation() {
        let short = TaskItem(id: "1", content: "Short task", status: .pending)
        XCTAssertEqual(short.truncatedContent, "Short task")

        let long = TaskItem(id: "2", content: String(repeating: "a", count: 60), status: .pending)
        XCTAssertEqual(long.truncatedContent.count, 50)
        XCTAssertTrue(long.truncatedContent.hasSuffix("..."))
    }

    func testSessionStateTaskCounts() {
        let session = SessionState(
            sessionId: "test", projectName: "Proj", gitBranch: "main",
            status: .working, model: "Opus", summary: "Testing",
            tasks: [
                TaskItem(id: "1", content: "Done", status: .completed),
                TaskItem(id: "2", content: "Doing", status: .inProgress),
                TaskItem(id: "3", content: "Todo", status: .pending),
            ]
        )
        XCTAssertEqual(session.completedTaskCount, 1)
        XCTAssertEqual(session.totalTaskCount, 3)
    }

    func testSessionStateTruncation() {
        let tasks = (1...15).map { TaskItem(id: "\($0)", content: "Task \($0)", status: .pending) }
        let session = SessionState(
            sessionId: "test", projectName: "Proj", gitBranch: "main",
            status: .working, model: "Opus", summary: "Testing", tasks: tasks
        )
        XCTAssertEqual(session.truncatedTasks.count, 10)
        XCTAssertEqual(session.truncatedTasks.first?.id, "6")
    }

    // MARK: - ModelInfo

    func testModelDisplayLabel() {
        XCTAssertEqual(ModelInfo.parse("claude-opus-4-6").displayLabel, "Opus 4.6")
        XCTAssertEqual(ModelInfo.parse("claude-opus-4-6[1m]").displayLabel, "Opus 4.6")
        XCTAssertEqual(ModelInfo.parse("claude-sonnet-4-6").displayLabel, "Sonnet 4.6")
        XCTAssertEqual(ModelInfo.parse("claude-haiku-4-5-20251001").displayLabel, "Haiku 4.5")
        XCTAssertEqual(ModelInfo.parse("claude-opus-4-1-20250805").displayLabel, "Opus 4.1")
        XCTAssertEqual(ModelInfo.parse("claude-3-5-sonnet-20241022").displayLabel, "Sonnet 3.5")
        XCTAssertEqual(ModelInfo.parse("unknown-model").displayLabel, "unknown-model")
    }

    func testModelContextWindow() {
        XCTAssertEqual(ModelInfo.parse("claude-opus-4-6").contextWindow, 1_000_000)
        XCTAssertEqual(ModelInfo.parse("claude-sonnet-4-6").contextWindow, 1_000_000)
        XCTAssertEqual(ModelInfo.parse("claude-haiku-4-5-20251001").contextWindow, 200_000)
        XCTAssertEqual(ModelInfo.parse("claude-opus-4-1-20250805").contextWindow, 200_000)
        XCTAssertEqual(ModelInfo.parse("claude-3-5-sonnet-20241022").contextWindow, 200_000)
        // [1m] suffix forces 1M even on older models
        XCTAssertEqual(ModelInfo.parse("claude-opus-4-1-20250805[1m]").contextWindow, 1_000_000)
    }

    func testModelDisplayLabelCompat() {
        // Existing function should delegate to ModelInfo
        XCTAssertEqual(modelDisplayLabel("claude-opus-4-6"), "Opus 4.6")
        XCTAssertEqual(modelDisplayLabel("claude-sonnet-4-6"), "Sonnet 4.6")
    }

    func testTaskStatusDecoding() throws {
        let json = #"{"id":"1","content":"test","status":"in_progress"}"#
        let task = try JSONDecoder().decode(TaskItem.self, from: json.data(using: .utf8)!)
        XCTAssertEqual(task.status, .inProgress)
    }
}
