import Foundation
import CodyncShared

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

// MARK: - Transcript Content Blocks

struct JSONLContentBlock: Codable {
    let type: String
    let id: String?
    let name: String?
    let text: String?
    let input: JSONLToolInput?
    let toolUseId: String?
    let isAsync: Bool?

    enum CodingKeys: String, CodingKey {
        case type, id, name, text, input
        case toolUseId = "tool_use_id"
        case isAsync
    }
}

struct JSONLToolInput: Codable {
    let command: String?
    let filePath: String?
    let pattern: String?

    enum CodingKeys: String, CodingKey {
        case command
        case filePath = "file_path"
        case pattern
    }
}

struct JSONLTranscriptMessage: Codable {
    let role: String?
    let model: String?
    let usage: JSONLUsage?
    let content: [JSONLContentBlock]?
    let stopReason: String?

    enum CodingKeys: String, CodingKey {
        case role, model, usage, content
        case stopReason = "stop_reason"
    }
}

struct JSONLProgressData: Codable {
    let type: String?
}

struct JSONLTranscriptEntry: Codable {
    let type: String?
    let subtype: String?
    let message: JSONLTranscriptMessage?
    let timestamp: String?
    let isMeta: Bool?
    let agentId: String?
    let promptId: String?
    let data: JSONLProgressData?
    /// Present on "last-prompt" type entries
    let lastPrompt: String?
}

struct JSONLSessionInfo {
    var model: String = "Unknown"
    var latestInputTokens: Int = 0  // Latest turn's input tokens (for context%)
    var totalOutputTokens: Int = 0   // Sum of all output tokens (for cost)

    var contextPct: Int {
        let contextWindowSize = modelContextSize
        guard contextWindowSize > 0, latestInputTokens > 0 else { return 0 }
        return min(100, (latestInputTokens * 100) / contextWindowSize)
    }

    var costUSD: Double {
        let (inputRate, outputRate) = modelPricing
        let inputCost = Double(latestInputTokens) * inputRate / 1_000_000
        let outputCost = Double(totalOutputTokens) * outputRate / 1_000_000
        return inputCost + outputCost
    }

    private var modelContextSize: Int {
        ModelInfo.parse(model).contextWindow
    }

    private var modelPricing: (Double, Double) {
        if model.contains("opus") { return (15.0, 75.0) }
        if model.contains("sonnet") { return (3.0, 15.0) }
        if model.contains("haiku") { return (0.25, 1.25) }
        return (3.0, 15.0)
    }
}

enum JSONLTailReader {
    static func readTail(url: URL, lineCount: Int = 50) -> [String] {
        guard let fileHandle = try? FileHandle(forReadingFrom: url) else { return [] }
        defer { try? fileHandle.close() }

        let fileSize = fileHandle.seekToEndOfFile()
        guard fileSize > 0 else { return [] }

        // ~4KB per line: JSONL entries with tool outputs average 3-5KB
        let chunkSize = min(fileSize, UInt64(lineCount * 4000))
        fileHandle.seek(toFileOffset: fileSize - chunkSize)

        guard let data = try? fileHandle.readToEnd(),
              let text = String(data: data, encoding: .utf8) else { return [] }

        let lines = text.components(separatedBy: "\n").filter { !$0.isEmpty }
        return Array(lines.suffix(lineCount))
    }

    static func extractInfo(url: URL) -> JSONLSessionInfo {
        let lines = readTail(url: url, lineCount: 100) // read more lines for better coverage
        var info = JSONLSessionInfo()
        var totalOutputTokens = 0

        let decoder = JSONDecoder()
        for line in lines {
            guard let data = line.data(using: .utf8),
                  let entry = try? decoder.decode(JSONLEntry.self, from: data) else { continue }

            if let message = entry.message {
                if let model = message.model, !model.isEmpty {
                    info.model = model
                }
                if let usage = message.usage {
                    // Context%: total context = input + cache_creation + cache_read
                    let inputTokens = usage.inputTokens ?? 0
                    let cacheCreation = usage.cacheCreationInputTokens ?? 0
                    let cacheRead = usage.cacheReadInputTokens ?? 0
                    if inputTokens > 0 {
                        info.latestInputTokens = inputTokens + cacheCreation + cacheRead
                    }
                    // Cost: accumulate output tokens
                    totalOutputTokens += usage.outputTokens ?? 0
                }
            }
        }

        info.totalOutputTokens = totalOutputTokens
        return info
    }
}
