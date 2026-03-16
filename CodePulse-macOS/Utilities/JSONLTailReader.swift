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

        let chunkSize = min(fileSize, UInt64(lineCount * 500))
        fileHandle.seek(toFileOffset: fileSize - chunkSize)

        guard let data = try? fileHandle.readToEnd(),
              let text = String(data: data, encoding: .utf8) else { return [] }

        let lines = text.components(separatedBy: "\n").filter { !$0.isEmpty }
        return Array(lines.suffix(lineCount))
    }

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

    static func fileSize(url: URL) -> UInt64? {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: url.path),
              let size = attrs[.size] as? UInt64 else { return nil }
        return size
    }
}
