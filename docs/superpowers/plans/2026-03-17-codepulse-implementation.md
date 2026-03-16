# CodePulse Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a macOS menu bar app that monitors Claude Code sessions and syncs progress to iPhone Live Activities via CloudKit.

**Architecture:** macOS app uses FSEvents to watch `~/.claude/` files, aggregates session state, syncs to CloudKit Private DB. iPhone app receives CloudKit push notifications and updates Live Activities on Dynamic Island and Lock Screen. Shared framework contains models and CloudKit logic.

**Tech Stack:** Swift, SwiftUI, AppKit (NSStatusItem/NSPopover), CloudKit, ActivityKit, FSEvents, XCTest

**Spec:** `docs/superpowers/specs/2026-03-17-codepulse-design.md`

---

## Chunk 1: Project Setup + Shared Models

### Task 1: Restructure Xcode Project

The current project is a single macOS target. We need to add iOS target, Live Activity extension, and shared framework. Since Xcode project files are binary-like and hard to edit programmatically, use `xcodegen` or manual Xcode operations.

**Files:**
- Modify: `CodePulse.xcodeproj/project.pbxproj` (via Xcode CLI or manual)
- Create: `CodePulseShared/` directory structure
- Create: `CodePulse-iOS/` directory structure
- Create: `CodePulseLiveActivity/` directory structure

- [ ] **Step 1: Create directory structure**

```bash
mkdir -p CodePulseShared/Models
mkdir -p CodePulseShared/CloudKit
mkdir -p CodePulse-iOS/App
mkdir -p CodePulse-iOS/Services
mkdir -p CodePulse-iOS/Views
mkdir -p CodePulseLiveActivity
mkdir -p CodePulse-macOS/App
mkdir -p CodePulse-macOS/Services
mkdir -p CodePulse-macOS/Views
mkdir -p CodePulse-macOS/Utilities
```

- [ ] **Step 2: Move existing macOS files**

```bash
mv CodePulse/CodePulseApp.swift CodePulse-macOS/App/
mv CodePulse/ContentView.swift CodePulse-macOS/Views/
mv CodePulse/Assets.xcassets CodePulse-macOS/
```

- [ ] **Step 3: Create Package.swift for shared framework**

Using Swift Package Manager for the shared code is cleaner than a framework target. Create a local package that both targets depend on.

Create `CodePulseShared/Package.swift`:
```swift
// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "CodePulseShared",
    platforms: [.macOS(.v14), .iOS(.v17)],
    products: [
        .library(name: "CodePulseShared", targets: ["CodePulseShared"]),
    ],
    targets: [
        .target(name: "CodePulseShared", path: "Sources"),
        .testTarget(name: "CodePulseSharedTests", dependencies: ["CodePulseShared"], path: "Tests"),
    ]
)
```

Restructure:
```bash
mkdir -p CodePulseShared/Sources/Models
mkdir -p CodePulseShared/Sources/CloudKit
mkdir -p CodePulseShared/Tests
```

- [ ] **Step 4: Update Xcode project**

In Xcode:
1. Remove old `CodePulse/` group reference
2. Add `CodePulse-macOS/` folder reference to macOS target
3. Add new target: iOS App (`CodePulse-iOS`, bundle ID `com.pokai.CodePulse.ios`, deployment target iOS 17.0)
4. Add new target: Widget Extension (`CodePulseLiveActivity`, bundle ID `com.pokai.CodePulse.ios.LiveActivity`)
5. Add `CodePulseShared` as local Swift Package dependency (File → Add Package → Add Local)
6. Add `CodePulseShared` to both macOS and iOS target dependencies
7. macOS target: disable App Sandbox in Signing & Capabilities
8. Both targets: add CloudKit capability with container `iCloud.com.pokai.CodePulse`
9. iOS target: add Push Notifications capability
10. iOS target: add Background Modes → Remote notifications

- [ ] **Step 5: Verify project builds**

```bash
xcodebuild -project CodePulse.xcodeproj -scheme CodePulse -destination 'platform=macOS' build
xcodebuild -project CodePulse.xcodeproj -scheme CodePulse-iOS -destination 'platform=iOS Simulator,name=iPhone 16' build
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: restructure project with macOS, iOS, LiveActivity targets and shared package"
```

### Task 2: Shared Models

**Files:**
- Create: `CodePulseShared/Sources/Models/SessionStatus.swift`
- Create: `CodePulseShared/Sources/Models/TaskItem.swift`
- Create: `CodePulseShared/Sources/Models/SessionState.swift`
- Test: `CodePulseShared/Tests/ModelsTests.swift`

- [ ] **Step 1: Write SessionStatus enum**

```swift
// CodePulseShared/Sources/Models/SessionStatus.swift
import SwiftUI

public enum SessionStatus: String, Codable, Sendable {
    case working
    case idle
    case needsInput
    case error
    case completed

    public var color: Color {
        switch self {
        case .working: return .green
        case .idle: return .cyan
        case .needsInput: return .orange
        case .error: return .red
        case .completed: return .gray
        }
    }

    public var label: String {
        switch self {
        case .working: return "Working"
        case .idle: return "Idle"
        case .needsInput: return "Needs Input"
        case .error: return "Error"
        case .completed: return "Completed"
        }
    }

    public var isActive: Bool {
        switch self {
        case .working, .idle, .needsInput, .error: return true
        case .completed: return false
        }
    }
}
```

- [ ] **Step 2: Write TaskItem model**

```swift
// CodePulseShared/Sources/Models/TaskItem.swift
import Foundation

public struct TaskItem: Codable, Hashable, Sendable, Identifiable {
    public let id: String
    public let content: String
    public let status: TaskStatus
    public let activeForm: String?

    public init(id: String, content: String, status: TaskStatus, activeForm: String? = nil) {
        self.id = id
        self.content = content
        self.status = status
        self.activeForm = activeForm
    }

    /// Truncate content for Live Activity 4KB limit
    public var truncatedContent: String {
        if content.count > 50 {
            return String(content.prefix(47)) + "..."
        }
        return content
    }
}

public enum TaskStatus: String, Codable, Sendable {
    case pending
    case inProgress = "in_progress"
    case completed
}
```

- [ ] **Step 3: Write SessionState model**

```swift
// CodePulseShared/Sources/Models/SessionState.swift
import Foundation

public struct SessionState: Codable, Identifiable, Sendable {
    public let sessionId: String
    public var projectName: String
    public var gitBranch: String
    public var status: SessionStatus
    public var model: String
    public var summary: String
    public var currentTask: String?
    public var tasks: [TaskItem]
    public var contextPct: Int
    public var costUSD: Double
    public var startedAt: Date
    public var durationSec: Int
    public var deviceId: String
    public var updatedAt: Date

    public var id: String { sessionId }

    public var completedTaskCount: Int {
        tasks.filter { $0.status == .completed }.count
    }

    public var totalTaskCount: Int {
        tasks.count
    }

    /// Tasks truncated to last 10 for Live Activity 4KB limit
    public var truncatedTasks: [TaskItem] {
        Array(tasks.suffix(10))
    }

    public init(
        sessionId: String, projectName: String, gitBranch: String,
        status: SessionStatus, model: String, summary: String,
        currentTask: String? = nil, tasks: [TaskItem] = [],
        contextPct: Int = 0, costUSD: Double = 0,
        startedAt: Date = Date(), durationSec: Int = 0,
        deviceId: String = "", updatedAt: Date = Date()
    ) {
        self.sessionId = sessionId
        self.projectName = projectName
        self.gitBranch = gitBranch
        self.status = status
        self.model = model
        self.summary = summary
        self.currentTask = currentTask
        self.tasks = tasks
        self.contextPct = contextPct
        self.costUSD = costUSD
        self.startedAt = startedAt
        self.durationSec = durationSec
        self.deviceId = deviceId
        self.updatedAt = updatedAt
    }
}
```

- [ ] **Step 4: Write model tests**

```swift
// CodePulseShared/Tests/ModelsTests.swift
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
        XCTAssertEqual(session.truncatedTasks.first?.id, "6") // last 10: 6-15
    }

    func testTaskStatusDecoding() throws {
        let json = #"{"id":"1","content":"test","status":"in_progress"}"#
        let task = try JSONDecoder().decode(TaskItem.self, from: json.data(using: .utf8)!)
        XCTAssertEqual(task.status, .inProgress)
    }
}
```

- [ ] **Step 5: Run tests**

```bash
cd CodePulseShared && swift test
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: add shared models — SessionState, TaskItem, SessionStatus"
```

---

## Chunk 2: macOS File Parsing

### Task 3: PID Checker + Claude Path Utilities

**Files:**
- Create: `CodePulse-macOS/Utilities/PIDChecker.swift`
- Create: `CodePulse-macOS/Utilities/ClaudePaths.swift`
- Test: `CodePulseTests/PIDCheckerTests.swift`
- Test: `CodePulseTests/ClaudePathsTests.swift`

- [ ] **Step 1: Write PIDChecker**

```swift
// CodePulse-macOS/Utilities/PIDChecker.swift
import Foundation

enum PIDChecker {
    /// Check if a process with the given PID is still alive
    static func isAlive(pid: Int) -> Bool {
        kill(Int32(pid), 0) == 0
    }
}
```

- [ ] **Step 2: Write ClaudePaths**

```swift
// CodePulse-macOS/Utilities/ClaudePaths.swift
import Foundation

enum ClaudePaths {
    static var claudeDir: URL {
        FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".claude")
    }

    static var sessionsDir: URL { claudeDir.appendingPathComponent("sessions") }
    static var tasksDir: URL { claudeDir.appendingPathComponent("tasks") }
    static var todosDir: URL { claudeDir.appendingPathComponent("todos") }
    static var projectsDir: URL { claudeDir.appendingPathComponent("projects") }

    /// Convert absolute path to Claude's mangled directory name
    /// e.g., "/Users/foo/myproject" → "-Users-foo-myproject"
    static func mangledCwd(_ cwd: String) -> String {
        cwd.replacingOccurrences(of: "/", with: "-")
    }

    /// Get session index path for a given working directory
    static func sessionIndexPath(cwd: String) -> URL {
        projectsDir
            .appendingPathComponent(mangledCwd(cwd))
            .appendingPathComponent("sessions-index.json")
    }

    /// Get JSONL conversation log path
    static func jsonlPath(cwd: String, sessionId: String) -> URL {
        projectsDir
            .appendingPathComponent(mangledCwd(cwd))
            .appendingPathComponent("\(sessionId).jsonl")
    }

    /// Get tasks directory for a session
    static func tasksPath(sessionId: String) -> URL {
        tasksDir.appendingPathComponent(sessionId)
    }

    /// Get todos glob pattern for a session
    static func todosPattern(sessionId: String) -> String {
        "\(sessionId)-agent-"
    }
}
```

- [ ] **Step 3: Write tests**

```swift
// CodePulseTests/ClaudePathsTests.swift
import XCTest
@testable import CodePulse

final class ClaudePathsTests: XCTestCase {
    func testMangledCwd() {
        XCTAssertEqual(ClaudePaths.mangledCwd("/Users/foo/myproject"), "-Users-foo-myproject")
        XCTAssertEqual(ClaudePaths.mangledCwd("/"), "-")
    }

    func testPIDCheckerCurrentProcess() {
        XCTAssertTrue(PIDChecker.isAlive(pid: Int(ProcessInfo.processInfo.processIdentifier)))
    }

    func testPIDCheckerDeadProcess() {
        XCTAssertFalse(PIDChecker.isAlive(pid: 999999))
    }
}
```

- [ ] **Step 4: Run tests, commit**

```bash
xcodebuild test -project CodePulse.xcodeproj -scheme CodePulse -destination 'platform=macOS'
git add -A && git commit -m "feat: add PIDChecker and ClaudePaths utilities"
```

### Task 4: Session File Parser

**Files:**
- Create: `CodePulse-macOS/Services/SessionFileParser.swift`
- Test: `CodePulseTests/SessionFileParserTests.swift`

- [ ] **Step 1: Write SessionFileParser**

Parses `~/.claude/sessions/<PID>.json`, `~/.claude/tasks/<sessionId>/`, `~/.claude/todos/<sessionId>-agent-*.json`, and `sessions-index.json`.

```swift
// CodePulse-macOS/Services/SessionFileParser.swift
import Foundation
import CodePulseShared

/// Raw session data from ~/.claude/sessions/<PID>.json
struct RawSessionFile: Codable {
    let pid: Int
    let sessionId: String
    let cwd: String
    let startedAt: Int64 // epoch ms
}

/// Entry from sessions-index.json
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

/// Raw task from ~/.claude/tasks/<sessionId>/<N>.json or todos JSON
struct RawTask: Codable {
    let content: String?
    let subject: String?
    let status: String
    let activeForm: String?
    let id: String?
    let description: String?
}

enum SessionFileParser {
    /// Parse active session files from ~/.claude/sessions/
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

    /// Parse tasks from ~/.claude/tasks/<sessionId>/ directory
    static func parseTasks(sessionId: String) -> [TaskItem] {
        let dir = ClaudePaths.tasksPath(sessionId: sessionId)
        guard let files = try? FileManager.default.contentsOfDirectory(at: dir, includingPropertiesForKeys: nil)
            .filter({ $0.pathExtension == "json" || $0.lastPathComponent != ".lock" && $0.lastPathComponent != ".highwatermark" }) else {
            return parseTodos(sessionId: sessionId) // fallback
        }

        // Read .highwatermark to know how many tasks
        let hwPath = dir.appendingPathComponent(".highwatermark")
        guard let hwData = try? String(contentsOf: hwPath, encoding: .utf8),
              let maxId = Int(hwData.trimmingCharacters(in: .whitespacesAndNewlines)) else {
            return parseTodos(sessionId: sessionId)
        }

        var tasks: [TaskItem] = []
        // Tasks directory contains per-task state; parse from available info
        // The actual task data is embedded in the JSONL, so we primarily use todos as the source
        let todoTasks = parseTodos(sessionId: sessionId)
        if !todoTasks.isEmpty { return todoTasks }

        // Fallback: create placeholder tasks based on highwatermark
        for i in 1...maxId {
            tasks.append(TaskItem(id: "\(i)", content: "Task \(i)", status: .pending))
        }
        return tasks
    }

    /// Parse todos from ~/.claude/todos/<sessionId>-agent-*.json (fallback)
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

    /// Parse session index to get summary and metadata
    static func parseSessionIndex(cwd: String, sessionId: String) -> SessionIndexEntry? {
        let path = ClaudePaths.sessionIndexPath(cwd: cwd)
        guard let data = try? Data(contentsOf: path),
              let index = try? JSONDecoder().decode(SessionIndex.self, from: data) else { return nil }
        return index.entries.first { $0.sessionId == sessionId }
    }
}
```

- [ ] **Step 2: Write tests with mock data**

```swift
// CodePulseTests/SessionFileParserTests.swift
import XCTest
@testable import CodePulse
@testable import CodePulseShared

final class SessionFileParserTests: XCTestCase {
    var tempDir: URL!

    override func setUp() {
        tempDir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try! FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: tempDir)
    }

    func testParseRawSessionFile() throws {
        let json = #"{"pid":12345,"sessionId":"abc-123","cwd":"/Users/test/project","startedAt":1773674000000}"#
        let data = json.data(using: .utf8)!
        let session = try JSONDecoder().decode(RawSessionFile.self, from: data)
        XCTAssertEqual(session.pid, 12345)
        XCTAssertEqual(session.sessionId, "abc-123")
        XCTAssertEqual(session.cwd, "/Users/test/project")
    }

    func testParseTodosJson() throws {
        let json = #"[{"content":"First task","status":"completed","activeForm":"Doing first"},{"content":"Second task","status":"in_progress","activeForm":"Doing second"},{"content":"Third task","status":"pending"}]"#
        let data = json.data(using: .utf8)!
        let rawTasks = try JSONDecoder().decode([RawTask].self, from: data)
        XCTAssertEqual(rawTasks.count, 3)
        XCTAssertEqual(rawTasks[0].status, "completed")
        XCTAssertEqual(rawTasks[1].status, "in_progress")
    }

    func testParseSessionIndex() throws {
        let json = #"{"version":1,"entries":[{"sessionId":"abc-123","firstPrompt":"fix the bug","summary":"Fixed auth bug","messageCount":10,"gitBranch":"main","projectPath":"/Users/test/project"}]}"#
        let data = json.data(using: .utf8)!
        let index = try JSONDecoder().decode(SessionIndex.self, from: data)
        XCTAssertEqual(index.entries.count, 1)
        XCTAssertEqual(index.entries[0].summary, "Fixed auth bug")
    }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
xcodebuild test -project CodePulse.xcodeproj -scheme CodePulse -destination 'platform=macOS'
git add -A && git commit -m "feat: add SessionFileParser for Claude Code data files"
```

### Task 5: JSONL Tail Reader

**Files:**
- Create: `CodePulse-macOS/Utilities/JSONLTailReader.swift`
- Test: `CodePulseTests/JSONLTailReaderTests.swift`

- [ ] **Step 1: Write JSONLTailReader**

Reads the last N lines of a JSONL file efficiently (seeks from end). Extracts model, token usage for context% and cost computation.

```swift
// CodePulse-macOS/Utilities/JSONLTailReader.swift
import Foundation

struct JSONLUsage: Codable {
    let inputTokens: Int?
    let outputTokens: Int?
    let cacheCreationInputTokens: Int?
    let cacheReadInputTokens: Int?

    enum CodingKeys: String, CodingKey {
        case inputTokens = "input_tokens"
        case outputTokens = "output_tokens"
        case cacheCreationInputTokens = "cache_creation_input_tokens"
        case cacheReadInputTokens = "cache_read_input_tokens"
    }
}

struct JSONLMessage: Codable {
    let role: String?
    let model: String?
    let usage: JSONLUsage?
}

struct JSONLEntry: Codable {
    let type: String?
    let message: JSONLMessage?
    let timestamp: String?
}

struct JSONLSessionInfo {
    var model: String = "Unknown"
    var totalInputTokens: Int = 0
    var totalOutputTokens: Int = 0
    var totalCacheReadTokens: Int = 0
    var lastUpdateTime: Date?

    var contextPct: Int {
        let contextWindowSize = modelContextSize
        guard contextWindowSize > 0 else { return 0 }
        let used = totalInputTokens + totalCacheReadTokens
        return min(100, (used * 100) / contextWindowSize)
    }

    var costUSD: Double {
        let (inputRate, outputRate) = modelPricing
        let inputCost = Double(totalInputTokens) * inputRate / 1_000_000
        let outputCost = Double(totalOutputTokens) * outputRate / 1_000_000
        return inputCost + outputCost
    }

    private var modelContextSize: Int {
        if model.contains("opus") { return 200_000 }
        if model.contains("sonnet") { return 200_000 }
        if model.contains("haiku") { return 200_000 }
        return 200_000
    }

    /// (input price per 1M tokens, output price per 1M tokens)
    private var modelPricing: (Double, Double) {
        if model.contains("opus") { return (15.0, 75.0) }
        if model.contains("sonnet") { return (3.0, 15.0) }
        if model.contains("haiku") { return (0.25, 1.25) }
        return (3.0, 15.0) // default to sonnet pricing
    }
}

enum JSONLTailReader {
    /// Read the last N lines of a file efficiently
    static func readTail(url: URL, lineCount: Int = 50) -> [String] {
        guard let fileHandle = try? FileHandle(forReadingFrom: url) else { return [] }
        defer { try? fileHandle.close() }

        let fileSize = fileHandle.seekToEndOfFile()
        guard fileSize > 0 else { return [] }

        // Read last chunk (estimate ~500 bytes per line)
        let chunkSize = min(fileSize, UInt64(lineCount * 500))
        fileHandle.seek(toFileOffset: fileSize - chunkSize)

        guard let data = try? fileHandle.readToEnd(),
              let text = String(data: data, encoding: .utf8) else { return [] }

        let lines = text.components(separatedBy: "\n").filter { !$0.isEmpty }
        return Array(lines.suffix(lineCount))
    }

    /// Extract session info (model, tokens, cost) from JSONL tail
    static func extractInfo(url: URL) -> JSONLSessionInfo {
        let lines = readTail(url: url)
        var info = JSONLSessionInfo()

        let decoder = JSONDecoder()
        for line in lines {
            guard let data = line.data(using: .utf8),
                  let entry = try? decoder.decode(JSONLEntry.self, from: data) else { continue }

            if let message = entry.message {
                if let model = message.model, !model.isEmpty {
                    info.model = model
                }
                if let usage = message.usage {
                    info.totalInputTokens += usage.inputTokens ?? 0
                    info.totalOutputTokens += usage.outputTokens ?? 0
                    info.totalCacheReadTokens += usage.cacheReadInputTokens ?? 0
                }
            }
        }

        return info
    }

    /// Check file size to detect growth (for status detection)
    static func fileSize(url: URL) -> UInt64? {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: url.path),
              let size = attrs[.size] as? UInt64 else { return nil }
        return size
    }
}
```

- [ ] **Step 2: Write tests**

```swift
// CodePulseTests/JSONLTailReaderTests.swift
import XCTest
@testable import CodePulse

final class JSONLTailReaderTests: XCTestCase {
    var tempFile: URL!

    override func setUp() {
        tempFile = FileManager.default.temporaryDirectory.appendingPathComponent("\(UUID()).jsonl")
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: tempFile)
    }

    func testReadTail() throws {
        let lines = (1...100).map { "line \($0)" }.joined(separator: "\n")
        try lines.write(to: tempFile, atomically: true, encoding: .utf8)

        let tail = JSONLTailReader.readTail(url: tempFile, lineCount: 5)
        XCTAssertEqual(tail.count, 5)
        XCTAssertEqual(tail.last, "line 100")
    }

    func testExtractModel() throws {
        let entries = [
            #"{"type":"assistant","message":{"role":"assistant","model":"claude-opus-4-6","usage":{"input_tokens":1000,"output_tokens":500}}}"#,
        ]
        try entries.joined(separator: "\n").write(to: tempFile, atomically: true, encoding: .utf8)

        let info = JSONLTailReader.extractInfo(url: tempFile)
        XCTAssertEqual(info.model, "claude-opus-4-6")
        XCTAssertEqual(info.totalInputTokens, 1000)
        XCTAssertEqual(info.totalOutputTokens, 500)
        XCTAssertTrue(info.contextPct > 0)
        XCTAssertTrue(info.costUSD > 0)
    }

    func testEmptyFile() throws {
        try "".write(to: tempFile, atomically: true, encoding: .utf8)
        let tail = JSONLTailReader.readTail(url: tempFile)
        XCTAssertTrue(tail.isEmpty)
    }
}
```

- [ ] **Step 3: Run tests, commit**

```bash
xcodebuild test -project CodePulse.xcodeproj -scheme CodePulse -destination 'platform=macOS'
git add -A && git commit -m "feat: add JSONLTailReader for efficient JSONL parsing"
```

---

## Chunk 3: macOS Core Services

### Task 6: SessionScanner (FSEvents watcher)

**Files:**
- Create: `CodePulse-macOS/Services/SessionScanner.swift`

- [ ] **Step 1: Write SessionScanner**

Uses DispatchSource.makeFileSystemObjectSource to watch directories for changes.

```swift
// CodePulse-macOS/Services/SessionScanner.swift
import Foundation
import Combine

@MainActor
final class SessionScanner: ObservableObject {
    @Published var activeSessions: [String: RawSessionFile] = [:] // sessionId -> RawSessionFile

    private var sessionsWatcher: DispatchSourceFileSystemObject?
    private var todoWatchers: [String: DispatchSourceFileSystemObject] = [:]
    private var scanTimer: Timer?
    private let debounceInterval: TimeInterval = 1.0

    func start() {
        // Initial scan
        scan()

        // Watch ~/.claude/sessions/ for changes
        watchDirectory(ClaudePaths.sessionsDir) { [weak self] in
            self?.scheduleScan()
        }

        // Periodic scan every 5 seconds as safety net
        scanTimer = Timer.scheduledTimer(withTimeInterval: 5.0, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.scan()
            }
        }
    }

    func stop() {
        sessionsWatcher?.cancel()
        sessionsWatcher = nil
        todoWatchers.values.forEach { $0.cancel() }
        todoWatchers.removeAll()
        scanTimer?.invalidate()
        scanTimer = nil
    }

    private func scheduleScan() {
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(debounceInterval))
            scan()
        }
    }

    func scan() {
        let files = SessionFileParser.parseSessionFiles()
        var newSessions: [String: RawSessionFile] = [:]

        for file in files {
            guard PIDChecker.isAlive(pid: file.pid) else { continue }
            newSessions[file.sessionId] = file
        }

        activeSessions = newSessions
    }

    private func watchDirectory(_ url: URL, handler: @escaping () -> Void) {
        let fd = open(url.path, O_EVTONLY)
        guard fd >= 0 else { return }

        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: fd,
            eventMask: [.write, .rename, .delete],
            queue: .main
        )
        source.setEventHandler(handler: handler)
        source.setCancelHandler { close(fd) }
        source.resume()
        sessionsWatcher = source
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add -A && git commit -m "feat: add SessionScanner with FSEvents directory watching"
```

### Task 7: SessionStateManager

**Files:**
- Create: `CodePulse-macOS/Services/SessionStateManager.swift`

- [ ] **Step 1: Write SessionStateManager**

Aggregates raw data from scanner into SessionState objects with status detection.

```swift
// CodePulse-macOS/Services/SessionStateManager.swift
import Foundation
import Combine
import CodePulseShared

@MainActor
final class SessionStateManager: ObservableObject {
    @Published var sessions: [SessionState] = []

    private let scanner: SessionScanner
    private var cancellables = Set<AnyCancellable>()
    private var jsonlSizes: [String: UInt64] = [:]
    private var jsonlSizeTimestamps: [String: Date] = [:]
    private let deviceId = Host.current().localizedName ?? UUID().uuidString

    init(scanner: SessionScanner) {
        self.scanner = scanner

        scanner.$activeSessions
            .debounce(for: .seconds(1), scheduler: RunLoop.main)
            .sink { [weak self] rawSessions in
                self?.updateSessions(from: rawSessions)
            }
            .store(in: &cancellables)
    }

    private func updateSessions(from rawSessions: [String: RawSessionFile]) {
        var updated: [SessionState] = []

        for (sessionId, raw) in rawSessions {
            let tasks = SessionFileParser.parseTasks(sessionId: sessionId)
            let indexEntry = SessionFileParser.parseSessionIndex(cwd: raw.cwd, sessionId: sessionId)
            let jsonlUrl = ClaudePaths.jsonlPath(cwd: raw.cwd, sessionId: sessionId)
            let jsonlInfo = JSONLTailReader.extractInfo(url: jsonlUrl)
            let status = detectStatus(raw: raw, tasks: tasks, jsonlUrl: jsonlUrl)

            let projectName = URL(fileURLWithPath: raw.cwd).lastPathComponent
            let startDate = Date(timeIntervalSince1970: TimeInterval(raw.startedAt) / 1000)
            let duration = Int(Date().timeIntervalSince(startDate))

            let currentTask = tasks.first(where: { $0.status == .inProgress })?.activeForm

            let summary = indexEntry?.summary
                ?? indexEntry?.firstPrompt?.prefix(50).description
                ?? projectName

            let session = SessionState(
                sessionId: sessionId,
                projectName: projectName,
                gitBranch: indexEntry?.gitBranch ?? "unknown",
                status: status,
                model: formatModel(jsonlInfo.model),
                summary: summary,
                currentTask: currentTask,
                tasks: tasks,
                contextPct: jsonlInfo.contextPct,
                costUSD: jsonlInfo.costUSD,
                startedAt: startDate,
                durationSec: duration,
                deviceId: deviceId,
                updatedAt: Date()
            )
            updated.append(session)
        }

        // Also check for recently completed sessions (PID died)
        for existing in sessions where existing.status != .completed {
            if !rawSessions.keys.contains(existing.sessionId) {
                var completed = existing
                completed.status = .completed
                completed.updatedAt = Date()
                updated.append(completed)
            }
        }

        sessions = updated.sorted { $0.startedAt > $1.startedAt }
    }

    private func detectStatus(raw: RawSessionFile, tasks: [TaskItem], jsonlUrl: URL) -> SessionStatus {
        guard PIDChecker.isAlive(pid: raw.pid) else { return .completed }

        if tasks.contains(where: { $0.status == .inProgress }) { return .working }

        // Check JSONL growth
        let currentSize = JSONLTailReader.fileSize(url: jsonlUrl) ?? 0
        let previousSize = jsonlSizes[raw.sessionId] ?? currentSize
        let lastChange = jsonlSizeTimestamps[raw.sessionId] ?? Date()

        if currentSize != previousSize {
            jsonlSizes[raw.sessionId] = currentSize
            jsonlSizeTimestamps[raw.sessionId] = Date()
            return .working
        }

        if Date().timeIntervalSince(lastChange) > 30 {
            return .needsInput
        }

        return .idle
    }

    private func formatModel(_ raw: String) -> String {
        if raw.contains("opus") { return "Opus" }
        if raw.contains("sonnet") { return "Sonnet" }
        if raw.contains("haiku") { return "Haiku" }
        return raw
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add -A && git commit -m "feat: add SessionStateManager with status detection and data aggregation"
```

---

## Chunk 4: macOS UI

### Task 8: Menu Bar Controller + App Entry

**Files:**
- Modify: `CodePulse-macOS/App/CodePulseApp.swift`
- Create: `CodePulse-macOS/App/MenuBarController.swift`

- [ ] **Step 1: Rewrite CodePulseApp as menu bar only app**

```swift
// CodePulse-macOS/App/CodePulseApp.swift
import SwiftUI

@main
struct CodePulseApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var delegate

    var body: some Scene {
        Settings { EmptyView() } // No main window
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    var menuBarController: MenuBarController?
    let scanner = SessionScanner()
    var stateManager: SessionStateManager!

    func applicationDidFinishLaunching(_ notification: Notification) {
        // Hide dock icon
        NSApp.setActivationPolicy(.accessory)

        stateManager = SessionStateManager(scanner: scanner)
        menuBarController = MenuBarController(stateManager: stateManager)
        scanner.start()
    }
}
```

- [ ] **Step 2: Write MenuBarController**

```swift
// CodePulse-macOS/App/MenuBarController.swift
import SwiftUI
import Combine
import CodePulseShared

final class MenuBarController: NSObject {
    private var statusItem: NSStatusItem!
    private var popover: NSPopover!
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager
        super.init()
        setupStatusItem()
        setupPopover()
        observeSessionCount()
    }

    private func setupStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "waveform.path", accessibilityDescription: "CodePulse")
            button.action = #selector(togglePopover)
            button.target = self
        }
    }

    private func setupPopover() {
        popover = NSPopover()
        popover.contentSize = NSSize(width: 340, height: 400)
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(
            rootView: SessionListView(stateManager: stateManager)
        )
    }

    private func observeSessionCount() {
        stateManager.$sessions
            .receive(on: RunLoop.main)
            .sink { [weak self] sessions in
                let activeCount = sessions.filter { $0.status.isActive }.count
                self?.updateBadge(count: activeCount)
            }
            .store(in: &cancellables)
    }

    private func updateBadge(count: Int) {
        guard let button = statusItem.button else { return }
        if count > 0 {
            button.title = " \(count)"
        } else {
            button.title = ""
        }
    }

    @objc private func togglePopover() {
        guard let button = statusItem.button else { return }
        if popover.isShown {
            popover.performClose(nil)
        } else {
            popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
            popover.contentViewController?.view.window?.makeKey()
        }
    }
}
```

- [ ] **Step 3: Delete old ContentView.swift, commit**

```bash
rm CodePulse-macOS/Views/ContentView.swift
git add -A && git commit -m "feat: add menu bar controller with popover and session count badge"
```

### Task 9: Status Dot View

**Files:**
- Create: `CodePulse-macOS/Views/StatusDotView.swift`

- [ ] **Step 1: Write StatusDotView**

```swift
// CodePulse-macOS/Views/StatusDotView.swift
import SwiftUI
import CodePulseShared

struct StatusDotView: View {
    let status: SessionStatus
    @State private var isAnimating = false

    var body: some View {
        Circle()
            .fill(status.color)
            .frame(width: 8, height: 8)
            .shadow(color: status.color.opacity(0.5), radius: shouldAnimate ? 3 : 0)
            .opacity(isAnimating ? 0.4 : 1.0)
            .animation(
                shouldAnimate ? .easeInOut(duration: 1.0).repeatForever(autoreverses: true) : .default,
                value: isAnimating
            )
            .onAppear {
                if shouldAnimate { isAnimating = true }
            }
            .onChange(of: status) { _, newStatus in
                isAnimating = newStatus == .working || newStatus == .needsInput || newStatus == .error
            }
    }

    private var shouldAnimate: Bool {
        status == .working || status == .needsInput || status == .error
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add -A && git commit -m "feat: add animated StatusDotView"
```

### Task 10: Session List + Row Views

**Files:**
- Create: `CodePulse-macOS/Views/SessionListView.swift`
- Create: `CodePulse-macOS/Views/SessionRowView.swift`

- [ ] **Step 1: Write SessionRowView**

```swift
// CodePulse-macOS/Views/SessionRowView.swift
import SwiftUI
import CodePulseShared

struct SessionRowView: View {
    let session: SessionState
    @State private var isHovered = false

    var body: some View {
        HStack(spacing: 8) {
            StatusDotView(status: session.status)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 4) {
                    Text(session.summary)
                        .font(.system(size: 13, weight: isHovered ? .medium : .regular))
                        .foregroundStyle(.primary)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    Text("claude")
                        .font(.system(size: 9))
                        .padding(.horizontal, 5)
                        .padding(.vertical, 1)
                        .background(Color.purple.opacity(0.15))
                        .foregroundStyle(.purple)
                        .clipShape(RoundedRectangle(cornerRadius: 3))
                }
            }

            Spacer(minLength: 4)

            Text(relativeTime(session.updatedAt))
                .font(.system(size: 11, design: .rounded))
                .foregroundStyle(.tertiary)
        }
        .padding(.leading, 6)
        .padding(.trailing, 10)
        .padding(.vertical, 5)
        .background(
            RoundedRectangle(cornerRadius: 5, style: .continuous)
                .fill(isHovered ? Color.primary.opacity(0.06) : Color.clear)
        )
        .onHover { isHovered = $0 }
        .contentShape(Rectangle())
    }

    private func relativeTime(_ date: Date) -> String {
        let seconds = -date.timeIntervalSinceNow
        if seconds < 60 { return "now" }
        if seconds < 3600 { return "\(Int(seconds / 60))m ago" }
        if seconds < 86400 { return "\(Int(seconds / 3600))h ago" }
        return "\(Int(seconds / 86400))d ago"
    }
}
```

- [ ] **Step 2: Write SessionListView**

```swift
// CodePulse-macOS/Views/SessionListView.swift
import SwiftUI
import CodePulseShared

struct SessionListView: View {
    @ObservedObject var stateManager: SessionStateManager
    @State private var selectedSession: SessionState?

    var body: some View {
        VStack(spacing: 0) {
            if let session = selectedSession {
                SessionDetailView(session: session) {
                    withAnimation { selectedSession = nil }
                }
            } else {
                sessionList
            }

            Divider().padding(.horizontal, 8)
            footer
        }
        .frame(width: 340)
    }

    private var sessionList: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                if stateManager.sessions.isEmpty {
                    emptyState
                } else {
                    ForEach(stateManager.sessions) { session in
                        Button {
                            withAnimation { selectedSession = session }
                        } label: {
                            SessionRowView(session: session)
                        }
                        .buttonStyle(.plain)

                        if session.id != stateManager.sessions.last?.id {
                            Divider().padding(.horizontal, 10)
                        }
                    }
                }
            }
            .padding(.vertical, 4)
        }
    }

    private var emptyState: some View {
        VStack(spacing: 8) {
            Image(systemName: "waveform.path")
                .font(.system(size: 32))
                .foregroundStyle(.tertiary)
            Text("No active Claude Code sessions")
                .font(.system(size: 13))
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, minHeight: 120)
    }

    private var footer: some View {
        HStack {
            Text("CodePulse")
                .font(.system(size: 10))
                .foregroundStyle(.quaternary)
            Spacer()
            Text("⌘.")
                .font(.system(size: 10))
                .foregroundStyle(.quaternary)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: add SessionListView and SessionRowView (Command-style)"
```

### Task 11: Session Detail View + Progress Bar

**Files:**
- Create: `CodePulse-macOS/Views/SessionDetailView.swift`
- Create: `CodePulse-macOS/Views/ProgressBarView.swift`

- [ ] **Step 1: Write ProgressBarView**

```swift
// CodePulse-macOS/Views/ProgressBarView.swift
import SwiftUI
import CodePulseShared

struct ProgressBarView: View {
    let tasks: [TaskItem]

    var body: some View {
        HStack(spacing: 3) {
            ForEach(tasks) { task in
                RoundedRectangle(cornerRadius: 3)
                    .fill(color(for: task.status))
                    .frame(height: 6)
                    .opacity(task.status == .inProgress ? 1.0 : 1.0)
                    .overlay(
                        task.status == .inProgress
                            ? RoundedRectangle(cornerRadius: 3)
                                .fill(color(for: task.status))
                                .opacity(0.4)
                                .animation(.easeInOut(duration: 1.0).repeatForever(autoreverses: true), value: true)
                            : nil
                    )
            }
        }
    }

    private func color(for status: TaskStatus) -> Color {
        switch status {
        case .completed: return .green
        case .inProgress: return .cyan
        case .pending: return Color(.separatorColor)
        }
    }
}
```

- [ ] **Step 2: Write SessionDetailView**

```swift
// CodePulse-macOS/Views/SessionDetailView.swift
import SwiftUI
import CodePulseShared

struct SessionDetailView: View {
    let session: SessionState
    let onBack: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Button(action: onBack) {
                    HStack(spacing: 4) {
                        Image(systemName: "chevron.left")
                        Text("Sessions")
                    }
                    .font(.system(size: 12))
                    .foregroundStyle(.cyan)
                }
                .buttonStyle(.plain)

                Spacer()

                StatusDotView(status: session.status)
                Text(session.status.label)
                    .font(.system(size: 11))
                    .foregroundStyle(session.status.color)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .background(Color.black.opacity(0.1))

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 10) {
                    // Title
                    VStack(alignment: .leading, spacing: 4) {
                        Text(session.summary)
                            .font(.system(size: 14, weight: .semibold))
                        HStack(spacing: 4) {
                            Text(session.projectName)
                            Text("·")
                            Text(session.gitBranch)
                            Text("·")
                            Text(session.model)
                        }
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 12)
                    .padding(.top, 8)

                    // Progress
                    if !session.tasks.isEmpty {
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text("TASKS")
                                    .font(.system(size: 10, weight: .medium))
                                    .foregroundStyle(.secondary)
                                Spacer()
                                Text("\(session.completedTaskCount) of \(session.totalTaskCount)")
                                    .font(.system(size: 10))
                                    .foregroundStyle(.secondary)
                            }

                            ProgressBarView(tasks: session.tasks)

                            ForEach(session.tasks) { task in
                                HStack(spacing: 6) {
                                    taskIcon(task.status)
                                    Text(task.content)
                                        .font(.system(size: 11))
                                        .foregroundStyle(taskColor(task.status))
                                    if task.status == .inProgress, let form = task.activeForm {
                                        Text("— \(form)")
                                            .font(.system(size: 11))
                                            .foregroundStyle(.quaternary)
                                    }
                                }
                            }
                        }
                        .padding(.horizontal, 12)
                    }

                    // Stats
                    HStack(spacing: 6) {
                        statCard("CONTEXT", "\(session.contextPct)%", .orange)
                        statCard("COST", String(format: "$%.2f", session.costUSD), .yellow)
                        statCard("DURATION", formatDuration(session.durationSec), .pink)
                    }
                    .padding(.horizontal, 12)
                    .padding(.bottom, 8)
                }
            }
        }
    }

    private func taskIcon(_ status: TaskStatus) -> some View {
        Group {
            switch status {
            case .completed: Text("✓").foregroundStyle(.green)
            case .inProgress: Text("◼").foregroundStyle(.cyan)
            case .pending: Text("◻").foregroundStyle(.quaternary)
            }
        }
        .font(.system(size: 11))
    }

    private func taskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: return .green
        case .inProgress: return .cyan
        case .pending: return Color(.tertiaryLabelColor)
        }
    }

    private func statCard(_ label: String, _ value: String, _ color: Color) -> some View {
        VStack(spacing: 2) {
            Text(label).font(.system(size: 9, weight: .medium)).foregroundStyle(.secondary)
            Text(value).font(.system(size: 12, weight: .semibold)).foregroundStyle(color)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 6)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color.black.opacity(0.15)))
    }

    private func formatDuration(_ seconds: Int) -> String {
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m" }
        return "\(seconds / 3600)h \((seconds % 3600) / 60)m"
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: add SessionDetailView with progress bar and task list"
```

---

## Chunk 5: CloudKit Sync

### Task 12: CloudKit Manager + Record Mapper

**Files:**
- Create: `CodePulseShared/Sources/CloudKit/CKRecordMapper.swift`
- Create: `CodePulseShared/Sources/CloudKit/CloudKitManager.swift`

- [ ] **Step 1: Write CKRecordMapper**

```swift
// CodePulseShared/Sources/CloudKit/CKRecordMapper.swift
import CloudKit
import Foundation

public enum CKRecordMapper {
    public static let recordType = "SessionState"

    public static func toRecord(_ session: SessionState, zoneID: CKRecordZone.ID = .default) -> CKRecord {
        let recordID = CKRecord.ID(recordName: session.sessionId, zoneID: zoneID)
        let record = CKRecord(recordType: recordType, recordID: recordID)

        record["sessionId"] = session.sessionId as CKRecordValue
        record["projectName"] = session.projectName as CKRecordValue
        record["gitBranch"] = session.gitBranch as CKRecordValue
        record["status"] = session.status.rawValue as CKRecordValue
        record["model"] = session.model as CKRecordValue
        record["summary"] = session.summary as CKRecordValue
        record["currentTask"] = (session.currentTask ?? "") as CKRecordValue
        record["contextPct"] = session.contextPct as CKRecordValue
        record["costUSD"] = session.costUSD as CKRecordValue
        record["startedAt"] = session.startedAt as CKRecordValue
        record["durationSec"] = session.durationSec as CKRecordValue
        record["deviceId"] = session.deviceId as CKRecordValue
        record["updatedAt"] = session.updatedAt as CKRecordValue

        if let tasksData = try? JSONEncoder().encode(session.truncatedTasks) {
            record["tasks"] = tasksData as CKRecordValue
        }

        return record
    }

    public static func fromRecord(_ record: CKRecord) -> SessionState? {
        guard let sessionId = record["sessionId"] as? String,
              let statusRaw = record["status"] as? String,
              let status = SessionStatus(rawValue: statusRaw) else { return nil }

        var tasks: [TaskItem] = []
        if let tasksData = record["tasks"] as? Data {
            tasks = (try? JSONDecoder().decode([TaskItem].self, from: tasksData)) ?? []
        }

        return SessionState(
            sessionId: sessionId,
            projectName: record["projectName"] as? String ?? "",
            gitBranch: record["gitBranch"] as? String ?? "",
            status: status,
            model: record["model"] as? String ?? "Unknown",
            summary: record["summary"] as? String ?? "",
            currentTask: record["currentTask"] as? String,
            tasks: tasks,
            contextPct: record["contextPct"] as? Int ?? 0,
            costUSD: record["costUSD"] as? Double ?? 0,
            startedAt: record["startedAt"] as? Date ?? Date(),
            durationSec: record["durationSec"] as? Int ?? 0,
            deviceId: record["deviceId"] as? String ?? "",
            updatedAt: record["updatedAt"] as? Date ?? Date()
        )
    }
}
```

- [ ] **Step 2: Write CloudKitManager**

```swift
// CodePulseShared/Sources/CloudKit/CloudKitManager.swift
import CloudKit
import Foundation

public final class CloudKitManager: Sendable {
    public static let shared = CloudKitManager()
    private let container = CKContainer(identifier: "iCloud.com.pokai.CodePulse")
    private var database: CKDatabase { container.privateCloudDatabase }

    private init() {}

    // MARK: - Write (macOS)

    public func save(_ session: SessionState) async throws {
        let record = CKRecordMapper.toRecord(session)
        _ = try await database.save(record)
    }

    public func saveIfChanged(_ session: SessionState, previous: SessionState?) async throws {
        guard session.updatedAt != previous?.updatedAt else { return }
        try await save(session)
    }

    // MARK: - Read (iOS)

    public func fetchAll() async throws -> [SessionState] {
        let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: NSPredicate(value: true))
        query.sortDescriptors = [NSSortDescriptor(key: "updatedAt", ascending: false)]

        let (results, _) = try await database.records(matching: query, resultsLimit: 20)
        return results.compactMap { _, result in
            guard case .success(let record) = result else { return nil }
            return CKRecordMapper.fromRecord(record)
        }
    }

    // MARK: - Subscribe (iOS)

    public func subscribeToChanges() async throws {
        let subscription = CKQuerySubscription(
            recordType: CKRecordMapper.recordType,
            predicate: NSPredicate(value: true),
            subscriptionID: "session-changes",
            options: [.firesOnRecordCreation, .firesOnRecordUpdate, .firesOnRecordDeletion]
        )

        let info = CKSubscription.NotificationInfo()
        info.shouldSendContentAvailable = true // silent push
        subscription.notificationInfo = info

        _ = try await database.save(subscription)
    }

    // MARK: - Cleanup

    public func deleteCompleted(olderThan hours: Int = 24) async throws {
        let cutoff = Date().addingTimeInterval(TimeInterval(-hours * 3600))
        let predicate = NSCompoundPredicate(andPredicateWithSubpredicates: [
            NSPredicate(format: "status == %@", "completed"),
            NSPredicate(format: "updatedAt < %@", cutoff as NSDate),
        ])
        let query = CKQuery(recordType: CKRecordMapper.recordType, predicate: predicate)
        let (results, _) = try await database.records(matching: query)

        for (id, _) in results {
            try await database.deleteRecord(withID: id)
        }
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: add CloudKitManager and CKRecordMapper for iCloud sync"
```

### Task 13: macOS CloudKit Sync Service

**Files:**
- Create: `CodePulse-macOS/Services/CloudKitSync.swift`

- [ ] **Step 1: Write CloudKitSync**

```swift
// CodePulse-macOS/Services/CloudKitSync.swift
import Foundation
import Combine
import CodePulseShared

@MainActor
final class CloudKitSync {
    private let stateManager: SessionStateManager
    private var cancellables = Set<AnyCancellable>()
    private var previousStates: [String: SessionState] = [:]
    private var syncTask: Task<Void, Never>?

    init(stateManager: SessionStateManager) {
        self.stateManager = stateManager

        stateManager.$sessions
            .debounce(for: .seconds(2), scheduler: RunLoop.main)
            .sink { [weak self] sessions in
                self?.syncToCloud(sessions)
            }
            .store(in: &cancellables)
    }

    private func syncToCloud(_ sessions: [SessionState]) {
        syncTask?.cancel()
        syncTask = Task {
            for session in sessions {
                let previous = previousStates[session.sessionId]
                do {
                    try await CloudKitManager.shared.saveIfChanged(session, previous: previous)
                    previousStates[session.sessionId] = session
                } catch {
                    print("CloudKit sync error for \(session.sessionId): \(error)")
                }
            }

            // Periodic cleanup
            try? await CloudKitManager.shared.deleteCompleted()
        }
    }
}
```

- [ ] **Step 2: Wire into AppDelegate**

Update `CodePulse-macOS/App/CodePulseApp.swift` AppDelegate:

```swift
// Add to AppDelegate:
var cloudKitSync: CloudKitSync!

// In applicationDidFinishLaunching, after stateManager init:
cloudKitSync = CloudKitSync(stateManager: stateManager)
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: add CloudKitSync service for macOS → iCloud push"
```

---

## Chunk 6: iOS App

### Task 14: iOS App Entry + CloudKit Receiver

**Files:**
- Create: `CodePulse-iOS/App/CodePulseIOSApp.swift`
- Create: `CodePulse-iOS/Services/CloudKitReceiver.swift`

- [ ] **Step 1: Write CloudKitReceiver**

```swift
// CodePulse-iOS/Services/CloudKitReceiver.swift
import Foundation
import CloudKit
import Combine
import CodePulseShared

@MainActor
final class CloudKitReceiver: ObservableObject {
    @Published var sessions: [SessionState] = []

    private var pollTimer: Timer?
    private var isLiveActivityActive = false

    func start() async {
        do {
            try await CloudKitManager.shared.subscribeToChanges()
        } catch {
            print("Failed to subscribe to CloudKit changes: \(error)")
        }
        await fetch()
    }

    func fetch() async {
        do {
            sessions = try await CloudKitManager.shared.fetchAll()
        } catch {
            print("Failed to fetch from CloudKit: \(error)")
        }
    }

    func onRemoteNotification() async {
        await fetch()
    }

    /// Start polling as fallback when Live Activity is active
    func startPolling() {
        guard pollTimer == nil else { return }
        isLiveActivityActive = true
        pollTimer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            Task { @MainActor in
                await self?.fetch()
            }
        }
    }

    func stopPolling() {
        isLiveActivityActive = false
        pollTimer?.invalidate()
        pollTimer = nil
    }
}
```

- [ ] **Step 2: Write iOS App entry**

```swift
// CodePulse-iOS/App/CodePulseIOSApp.swift
import SwiftUI
import CodePulseShared

@main
struct CodePulseIOSApp: App {
    @StateObject private var receiver = CloudKitReceiver()
    @StateObject private var liveActivityManager = LiveActivityManager()
    @UIApplicationDelegateAdaptor(IOSAppDelegate.self) var delegate

    var body: some Scene {
        WindowGroup {
            IOSRootView(receiver: receiver, liveActivityManager: liveActivityManager)
                .task { await receiver.start() }
        }
    }
}

class IOSAppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        application.registerForRemoteNotifications()
        return true
    }

    func application(_ application: UIApplication, didReceiveRemoteNotification userInfo: [AnyHashable: Any]) async -> UIBackgroundFetchResult {
        // Will be wired to CloudKitReceiver
        return .newData
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: add iOS app entry and CloudKitReceiver"
```

### Task 15: iOS Views

**Files:**
- Create: `CodePulse-iOS/Views/IOSRootView.swift`
- Create: `CodePulse-iOS/Views/IOSSessionListView.swift`
- Create: `CodePulse-iOS/Views/IOSSessionDetailView.swift`
- Create: `CodePulse-iOS/Views/IOSOnboardingView.swift`

- [ ] **Step 1: Write IOSRootView**

```swift
// CodePulse-iOS/Views/IOSRootView.swift
import SwiftUI
import CodePulseShared

struct IOSRootView: View {
    @ObservedObject var receiver: CloudKitReceiver
    @ObservedObject var liveActivityManager: LiveActivityManager

    var body: some View {
        NavigationStack {
            if receiver.sessions.isEmpty {
                IOSOnboardingView()
            } else {
                IOSSessionListView(
                    sessions: receiver.sessions,
                    liveActivityManager: liveActivityManager
                )
            }
        }
    }
}
```

- [ ] **Step 2: Write IOSOnboardingView**

```swift
// CodePulse-iOS/Views/IOSOnboardingView.swift
import SwiftUI

struct IOSOnboardingView: View {
    var body: some View {
        VStack(spacing: 20) {
            Image(systemName: "waveform.path")
                .font(.system(size: 64))
                .foregroundStyle(.cyan)

            Text("CodePulse")
                .font(.title.bold())

            Text("Install CodePulse on your Mac\nto start monitoring Claude Code sessions")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)

            HStack(spacing: 8) {
                Circle().fill(.green).frame(width: 8, height: 8)
                Text("Waiting for connection...")
                    .foregroundStyle(.secondary)
                    .font(.callout)
            }
            .padding(.top, 20)
        }
        .padding()
    }
}
```

- [ ] **Step 3: Write IOSSessionListView**

```swift
// CodePulse-iOS/Views/IOSSessionListView.swift
import SwiftUI
import CodePulseShared

struct IOSSessionListView: View {
    let sessions: [SessionState]
    @ObservedObject var liveActivityManager: LiveActivityManager

    var body: some View {
        List(sessions) { session in
            NavigationLink(destination: IOSSessionDetailView(
                session: session, liveActivityManager: liveActivityManager
            )) {
                HStack(spacing: 10) {
                    Circle()
                        .fill(session.status.color)
                        .frame(width: 10, height: 10)

                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            Text(session.summary)
                                .font(.body.weight(.medium))
                                .lineLimit(1)

                            if liveActivityManager.trackedSessionId == session.sessionId {
                                Text("LIVE")
                                    .font(.system(size: 9, weight: .semibold))
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(Color.green.opacity(0.15))
                                    .foregroundStyle(.green)
                                    .clipShape(RoundedRectangle(cornerRadius: 4))
                            }
                        }

                        Text("\(session.projectName) · \(session.completedTaskCount)/\(session.totalTaskCount) tasks · \(session.status.label)")
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        if !session.tasks.isEmpty {
                            IOSMiniProgressBar(tasks: session.tasks)
                        }
                    }
                }
                .padding(.vertical, 4)
            }
        }
        .navigationTitle("CodePulse")
    }
}

struct IOSMiniProgressBar: View {
    let tasks: [TaskItem]

    var body: some View {
        HStack(spacing: 2) {
            ForEach(tasks) { task in
                RoundedRectangle(cornerRadius: 2)
                    .fill(color(for: task.status))
                    .frame(height: 3)
            }
        }
        .padding(.top, 2)
    }

    private func color(for status: TaskStatus) -> Color {
        switch status {
        case .completed: return .green
        case .inProgress: return .cyan
        case .pending: return Color(.systemGray4)
        }
    }
}
```

- [ ] **Step 4: Write IOSSessionDetailView**

```swift
// CodePulse-iOS/Views/IOSSessionDetailView.swift
import SwiftUI
import CodePulseShared

struct IOSSessionDetailView: View {
    let session: SessionState
    @ObservedObject var liveActivityManager: LiveActivityManager

    private var isTracked: Bool {
        liveActivityManager.trackedSessionId == session.sessionId
    }

    var body: some View {
        List {
            // Status
            Section {
                HStack {
                    Circle().fill(session.status.color).frame(width: 10, height: 10)
                    Text(session.status.label)
                    Spacer()
                    Text(session.model).foregroundStyle(.secondary)
                }

                HStack {
                    Text(session.projectName)
                    Spacer()
                    Text(session.gitBranch).foregroundStyle(.secondary)
                }
            }

            // Tasks
            if !session.tasks.isEmpty {
                Section("Tasks — \(session.completedTaskCount) of \(session.totalTaskCount)") {
                    IOSMiniProgressBar(tasks: session.tasks)
                        .padding(.vertical, 4)

                    ForEach(session.tasks) { task in
                        HStack(spacing: 8) {
                            taskIcon(task.status)
                            Text(task.content)
                                .foregroundStyle(task.status == .pending ? .secondary : .primary)
                        }
                    }
                }
            }

            // Stats
            Section {
                HStack {
                    statItem("Context", "\(session.contextPct)%")
                    Divider()
                    statItem("Cost", String(format: "$%.2f", session.costUSD))
                    Divider()
                    statItem("Time", formatDuration(session.durationSec))
                }
                .padding(.vertical, 4)
            }

            // Live Activity
            Section {
                Toggle("Live Activity", isOn: Binding(
                    get: { isTracked },
                    set: { newValue in
                        if newValue {
                            liveActivityManager.startTracking(session)
                        } else {
                            liveActivityManager.stopTracking()
                        }
                    }
                ))

                if isTracked {
                    Text("Showing on Dynamic Island and Lock Screen")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            } footer: {
                Text("Live Activity will auto-switch to the next active session when this one completes.")
            }
        }
        .navigationTitle(session.summary)
        .navigationBarTitleDisplayMode(.inline)
    }

    private func taskIcon(_ status: TaskStatus) -> some View {
        Group {
            switch status {
            case .completed: Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
            case .inProgress: Image(systemName: "circle.dotted.circle").foregroundStyle(.cyan)
            case .pending: Image(systemName: "circle").foregroundStyle(.secondary)
            }
        }
        .font(.system(size: 14))
    }

    private func statItem(_ label: String, _ value: String) -> some View {
        VStack(spacing: 2) {
            Text(label).font(.caption2).foregroundStyle(.secondary)
            Text(value).font(.callout.weight(.semibold))
        }
        .frame(maxWidth: .infinity)
    }

    private func formatDuration(_ seconds: Int) -> String {
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m" }
        return "\(seconds / 3600)h \((seconds % 3600) / 60)m"
    }
}
```

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat: add iOS views — session list, detail, onboarding"
```

---

## Chunk 7: Live Activity Extension

### Task 16: Live Activity Manager

**Files:**
- Create: `CodePulse-iOS/Services/LiveActivityManager.swift`

- [ ] **Step 1: Write LiveActivityManager**

```swift
// CodePulse-iOS/Services/LiveActivityManager.swift
import ActivityKit
import Foundation
import Combine
import UserNotifications
import CodePulseShared

@MainActor
final class LiveActivityManager: ObservableObject {
    @Published var trackedSessionId: String?
    private var currentActivity: Activity<CodePulseAttributes>?

    func startTracking(_ session: SessionState) {
        stopTracking()

        let attributes = CodePulseAttributes(
            sessionId: session.sessionId,
            projectName: session.projectName
        )
        let state = contentState(from: session)

        do {
            currentActivity = try Activity.request(
                attributes: attributes,
                content: .init(state: state, staleDate: nil),
                pushType: nil
            )
            trackedSessionId = session.sessionId
        } catch {
            print("Failed to start Live Activity: \(error)")
        }
    }

    func stopTracking() {
        Task {
            await currentActivity?.end(nil, dismissalPolicy: .immediate)
        }
        currentActivity = nil
        trackedSessionId = nil
    }

    func update(sessions: [SessionState]) {
        guard let trackedId = trackedSessionId else {
            // Auto-track most recent active session
            if let newest = sessions.first(where: { $0.status == .working }) {
                startTracking(newest)
            }
            return
        }

        guard let session = sessions.first(where: { $0.sessionId == trackedId }) else { return }

        if session.status == .completed {
            onSessionCompleted(session, allSessions: sessions)
        } else {
            let state = contentState(from: session)
            Task {
                await currentActivity?.update(.init(state: state, staleDate: nil))
            }
        }
    }

    private func onSessionCompleted(_ session: SessionState, allSessions: [SessionState]) {
        // Send completion notification with haptic
        sendCompletionNotification(session)

        // Find next active session
        if let next = allSessions.first(where: { $0.status == .working && $0.sessionId != session.sessionId }) {
            startTracking(next)
        } else {
            // All done
            let total = allSessions.reduce(0.0) { $0 + $1.costUSD }
            sendAllCompleteNotification(count: allSessions.count, totalCost: total)
            stopTracking()
        }
    }

    private func contentState(from session: SessionState) -> CodePulseAttributes.ContentState {
        .init(
            status: session.status.rawValue,
            model: session.model,
            tasks: session.truncatedTasks,
            completedCount: session.completedTaskCount,
            totalCount: session.totalTaskCount,
            currentTask: session.currentTask,
            contextPct: session.contextPct,
            costUSD: session.costUSD,
            durationSec: session.durationSec
        )
    }

    private func sendCompletionNotification(_ session: SessionState) {
        let content = UNMutableNotificationContent()
        content.title = "Session Complete"
        content.body = "\(session.summary) finished · $\(String(format: "%.2f", session.costUSD))"
        content.sound = .default

        let request = UNNotificationRequest(identifier: session.sessionId, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }

    private func sendAllCompleteNotification(count: Int, totalCost: Double) {
        let content = UNMutableNotificationContent()
        content.title = "All Sessions Complete"
        content.body = "All \(count) sessions finished · Total $\(String(format: "%.2f", totalCost))"
        content.sound = .default

        let request = UNNotificationRequest(identifier: "all-complete", content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add -A && git commit -m "feat: add LiveActivityManager with auto-switch and notifications"
```

### Task 17: Live Activity Widget Extension

**Files:**
- Create: `CodePulseLiveActivity/CodePulseAttributes.swift` (in shared, but Activity-specific)
- Create: `CodePulseLiveActivity/CodePulseLiveActivity.swift`

- [ ] **Step 1: Write CodePulseAttributes**

This goes in the shared package since both iOS app and widget extension need it.

```swift
// CodePulseShared/Sources/Models/CodePulseAttributes.swift
import ActivityKit
import Foundation

public struct CodePulseAttributes: ActivityAttributes {
    public let sessionId: String
    public let projectName: String

    public init(sessionId: String, projectName: String) {
        self.sessionId = sessionId
        self.projectName = projectName
    }

    public struct ContentState: Codable, Hashable {
        public let status: String
        public let model: String
        public let tasks: [TaskItem]
        public let completedCount: Int
        public let totalCount: Int
        public let currentTask: String?
        public let contextPct: Int
        public let costUSD: Double
        public let durationSec: Int

        public init(status: String, model: String, tasks: [TaskItem],
                    completedCount: Int, totalCount: Int, currentTask: String?,
                    contextPct: Int, costUSD: Double, durationSec: Int) {
            self.status = status
            self.model = model
            self.tasks = tasks
            self.completedCount = completedCount
            self.totalCount = totalCount
            self.currentTask = currentTask
            self.contextPct = contextPct
            self.costUSD = costUSD
            self.durationSec = durationSec
        }
    }
}
```

- [ ] **Step 2: Write Live Activity UI**

```swift
// CodePulseLiveActivity/CodePulseLiveActivity.swift
import WidgetKit
import SwiftUI
import CodePulseShared

struct CodePulseLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: CodePulseAttributes.self) { context in
            // Lock Screen view
            LockScreenView(context: context)
        } dynamicIsland: { context in
            DynamicIsland {
                // Expanded
                DynamicIslandExpandedRegion(.leading) {
                    HStack(spacing: 4) {
                        statusDot(context.state.status)
                        Text(context.attributes.projectName)
                            .font(.caption2.weight(.semibold))
                            .lineLimit(1)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(context.state.model)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(spacing: 4) {
                        miniProgressBar(context.state)
                        HStack {
                            if let task = context.state.currentTask {
                                Text("◼ \(task)")
                                    .font(.caption2)
                                    .foregroundStyle(.cyan)
                                    .lineLimit(1)
                            }
                            Spacer()
                            Text("\(context.state.completedCount)/\(context.state.totalCount) · $\(String(format: "%.2f", context.state.costUSD))")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            } compactLeading: {
                HStack(spacing: 3) {
                    statusDot(context.state.status)
                    miniProgressBar(context.state)
                }
            } compactTrailing: {
                Text("\(context.state.completedCount)/\(context.state.totalCount)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } minimal: {
                statusDot(context.state.status)
            }
        }
    }

    private func statusDot(_ status: String) -> some View {
        Circle()
            .fill(statusColor(status))
            .frame(width: 6, height: 6)
    }

    private func statusColor(_ status: String) -> Color {
        switch status {
        case "working": return .green
        case "idle": return .cyan
        case "needsInput": return .orange
        case "error": return .red
        default: return .gray
        }
    }

    private func miniProgressBar(_ state: CodePulseAttributes.ContentState) -> some View {
        HStack(spacing: 2) {
            ForEach(Array(state.tasks.prefix(10).enumerated()), id: \.offset) { _, task in
                RoundedRectangle(cornerRadius: 2)
                    .fill(taskColor(task.status))
                    .frame(height: 4)
            }
        }
    }

    private func taskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: return .green
        case .inProgress: return .cyan
        case .pending: return Color(.systemGray3)
        }
    }
}

struct LockScreenView: View {
    let context: ActivityViewContext<CodePulseAttributes>

    var body: some View {
        VStack(spacing: 6) {
            HStack {
                HStack(spacing: 4) {
                    Image(systemName: "waveform.path")
                        .font(.caption2)
                    Text(context.attributes.projectName)
                        .font(.callout.weight(.medium))
                }
                Spacer()
                HStack(spacing: 4) {
                    Circle()
                        .fill(statusColor(context.state.status))
                        .frame(width: 6, height: 6)
                    Text(statusLabel(context.state.status))
                        .font(.caption)
                        .foregroundStyle(statusColor(context.state.status))
                }
            }

            // Progress bar
            HStack(spacing: 2) {
                ForEach(Array(context.state.tasks.prefix(10).enumerated()), id: \.offset) { _, task in
                    RoundedRectangle(cornerRadius: 3)
                        .fill(taskColor(task.status))
                        .frame(height: 6)
                }
            }

            HStack {
                if let task = context.state.currentTask {
                    Text("◼ \(task)")
                        .font(.caption)
                        .foregroundStyle(.cyan)
                        .lineLimit(1)
                }
                Spacer()
                Text("\(context.state.completedCount)/\(context.state.totalCount) tasks")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(12)
    }

    private func statusColor(_ status: String) -> Color {
        switch status {
        case "working": return .green
        case "idle": return .cyan
        case "needsInput": return .orange
        case "error": return .red
        default: return .gray
        }
    }

    private func statusLabel(_ status: String) -> String {
        switch status {
        case "working": return "Working"
        case "idle": return "Idle"
        case "needsInput": return "Needs Input"
        case "error": return "Error"
        default: return "Done"
        }
    }

    private func taskColor(_ status: TaskStatus) -> Color {
        switch status {
        case .completed: return .green
        case .inProgress: return .cyan
        case .pending: return Color(.systemGray3)
        }
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: add Live Activity extension with Dynamic Island and Lock Screen"
```

---

## Chunk 8: Integration + Wire Up

### Task 18: Wire iOS remote notifications to CloudKitReceiver

**Files:**
- Modify: `CodePulse-iOS/App/CodePulseIOSApp.swift`

- [ ] **Step 1: Connect remote notification to receiver and LiveActivityManager**

Update IOSAppDelegate to forward remote notifications and connect LiveActivityManager to receiver updates:

```swift
// In IOSRootView, add .onChange:
.onChange(of: receiver.sessions) { _, sessions in
    liveActivityManager.update(sessions: sessions)
}
```

Wire the AppDelegate remote notification to receiver (requires passing receiver reference via environment or shared instance).

- [ ] **Step 2: Request notification permissions in IOSRootView onAppear**

```swift
.task {
    let center = UNUserNotificationCenter.current()
    try? await center.requestAuthorization(options: [.alert, .sound, .badge])
}
```

- [ ] **Step 3: Commit**

```bash
git add -A && git commit -m "feat: wire iOS remote notifications and connect Live Activity updates"
```

### Task 19: End-to-End Smoke Test

- [ ] **Step 1: Build both targets**

```bash
xcodebuild -project CodePulse.xcodeproj -scheme CodePulse -destination 'platform=macOS' build
xcodebuild -project CodePulse.xcodeproj -scheme CodePulse-iOS -destination 'platform=iOS Simulator,name=iPhone 16' build
```

- [ ] **Step 2: Manual smoke test on macOS**

1. Launch CodePulse on Mac
2. Open a Claude Code session in terminal
3. Verify session appears in menu bar popover
4. Verify task progress updates in real-time
5. Verify status dot changes with session state

- [ ] **Step 3: Manual smoke test on iOS Simulator**

1. Launch CodePulse on iOS Simulator
2. Verify onboarding screen shows when no data
3. (CloudKit requires real device — mark for device testing)

- [ ] **Step 4: Final commit**

```bash
git add -A && git commit -m "feat: CodePulse v0.1.0 — macOS menu bar + iOS Live Activity"
```
