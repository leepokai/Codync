import Foundation

/// One grounded answer held only during the call. Suggestions remain Google's original HTML.
struct CallSearchResult: Equatable {
    let html: String
    let text: String
}

/// Joins transcription and grounding metadata for the same model turn. Transcription is not ordered with the other
/// server messages, so after `turnComplete` the voice's words alone can't say which answer they belong to: the next
/// turn starts when the user is heard or the next answer's model content arrives, whichever comes first. Words that
/// arrive before either stay with the answer that just finished.
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
        let heard = (content["inputTranscription"] as? [String: Any])?["text"] is String
        let next = complete && (content["modelTurn"] != nil || heard)
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
