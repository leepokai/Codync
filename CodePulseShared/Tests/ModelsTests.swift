import XCTest
@testable import CodePulseShared

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

    func testTaskStatusDecoding() throws {
        let json = #"{"id":"1","content":"test","status":"in_progress"}"#
        let task = try JSONDecoder().decode(TaskItem.self, from: json.data(using: .utf8)!)
        XCTAssertEqual(task.status, .inProgress)
    }
}
