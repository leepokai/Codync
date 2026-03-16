import Foundation
import CodePulseShared

struct RawSessionFile: Codable {
    let pid: Int
    let sessionId: String
    let cwd: String
    let startedAt: Int64
}

struct SessionIndexEntry: Codable {
    let sessionId: String
    let firstPrompt: String?
    let summary: String?
    let messageCount: Int?
    let gitBranch: String?
    let projectPath: String?
}

struct SessionIndex: Codable {
    let version: Int
    let entries: [SessionIndexEntry]
}

struct RawTask: Codable {
    let content: String?
    let subject: String?
    let status: String
    let activeForm: String?
    let id: String?
    let description: String?
}

enum SessionFileParser {
    static func parseSessionFiles() -> [RawSessionFile] {
        let dir = ClaudePaths.sessionsDir
        guard let files = try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
            .filter({ $0.pathExtension == "json" }) else { return [] }

        return files.compactMap { url in
            guard let data = try? Data(contentsOf: url),
                  let session = try? JSONDecoder().decode(RawSessionFile.self, from: data) else { return nil }
            return session
        }
    }

    static func parseTasks(sessionId: String) -> [TaskItem] {
        let todoTasks = parseTodos(sessionId: sessionId)
        if !todoTasks.isEmpty { return todoTasks }

        // Fallback: check tasks directory for highwatermark
        let dir = ClaudePaths.tasksPath(sessionId: sessionId)
        let hwPath = dir.appendingPathComponent(".highwatermark")
        guard let hwData = try? String(contentsOf: hwPath, encoding: .utf8),
              let maxId = Int(hwData.trimmingCharacters(in: .whitespacesAndNewlines)) else {
            return []
        }

        return (1...maxId).map {
            TaskItem(id: "\($0)", content: "Task \($0)", status: .pending)
        }
    }

    static func parseTodos(sessionId: String) -> [TaskItem] {
        let dir = ClaudePaths.todosDir
        guard let files = try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil) else { return [] }

        let matching = files.filter { $0.lastPathComponent.hasPrefix(ClaudePaths.todosPattern(sessionId: sessionId)) }

        for file in matching {
            guard let data = try? Data(contentsOf: file),
                  let rawTasks = try? JSONDecoder().decode([RawTask].self, from: data),
                  !rawTasks.isEmpty else { continue }

            return rawTasks.enumerated().map { index, raw in
                let status: TaskStatus = switch raw.status {
                case "completed": .completed
                case "in_progress": .inProgress
                default: .pending
                }
                return TaskItem(
                    id: raw.id ?? "\(index + 1)",
                    content: raw.subject ?? raw.content ?? "Task \(index + 1)",
                    status: status,
                    activeForm: raw.activeForm
                )
            }
        }
        return []
    }

    static func parseSessionIndex(cwd: String, sessionId: String) -> SessionIndexEntry? {
        let path = ClaudePaths.sessionIndexPath(cwd: cwd)
        guard let data = try? Data(contentsOf: path),
              let index = try? JSONDecoder().decode(SessionIndex.self, from: data) else { return nil }
        return index.entries.first { $0.sessionId == sessionId }
    }
}
