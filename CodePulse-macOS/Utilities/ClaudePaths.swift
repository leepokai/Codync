import Foundation

enum ClaudePaths {
    static var claudeDir: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".claude")
    }

    static var sessionsDir: URL { claudeDir.appendingPathComponent("sessions") }
    static var tasksDir: URL { claudeDir.appendingPathComponent("tasks") }
    static var todosDir: URL { claudeDir.appendingPathComponent("todos") }
    static var projectsDir: URL { claudeDir.appendingPathComponent("projects") }

    static func mangledCwd(_ cwd: String) -> String {
        cwd.replacingOccurrences(of: "/", with: "-")
    }

    static func sessionIndexPath(cwd: String) -> URL {
        projectsDir
            .appendingPathComponent(mangledCwd(cwd))
            .appendingPathComponent("sessions-index.json")
    }

    static func jsonlPath(cwd: String, sessionId: String) -> URL {
        projectsDir
            .appendingPathComponent(mangledCwd(cwd))
            .appendingPathComponent("\(sessionId).jsonl")
    }

    static func todosPattern(sessionId: String) -> String {
        "\(sessionId)-agent-"
    }
}
