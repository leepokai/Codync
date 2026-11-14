import Foundation

/// One grounded answer held only during the call. Suggestions remain Google's original HTML.
struct CallSearchResult: Equatable {
    let html: String
    let text: String
}

/// Joins transcription and grounding metadata for the same model turn. Transcription is not ordered with the other
/// server messages, so a piece that arrives after `turnComplete` still belongs to that turn: only the next turn's
/// model content starts a new one.
struct CallSearchTurn {
    private(set) var result: CallSearchResult?
    /// Turns that came with search suggestions.
    private(set) var answers = 0
    private var text = ""
    private var html: String?
    private var complete = false

    mutating func receive(_ content: [String: Any]) -> CallSearchResult? {
        let text = (content["outputTranscription"] as? [String: Any])?["text"] as? String
        let metadata = content["groundingMetadata"] as? [String: Any]
        let html = (metadata?["searchEntryPoint"] as? [String: Any])?["renderedContent"] as? String
        let next = complete && content["modelTurn"] != nil
        if next { self.text = ""; self.html = nil; complete = false }
        if let text { self.text += text }
        if let html, !html.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            if self.html == nil { answers += 1 }
            self.html = html
        }
        if next || text != nil || html != nil {
            result = self.html.map { CallSearchResult(html: $0, text: self.text) }
        }
        if content["turnComplete"] as? Bool == true || content["interrupted"] as? Bool == true { complete = true }
        return result
    }
}
