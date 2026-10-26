import Foundation

/// One grounded answer held only during the call. Suggestions remain Google's original HTML.
struct CallSearchResult: Equatable {
    let html: String
    let text: String
}

/// Transcription and metadata can arrive separately, in either order, during a model turn.
struct CallSearchTurn {
    private(set) var result: CallSearchResult?
    private var text = ""
    private var html: String?
    private var complete = true

    mutating func receive(_ content: [String: Any]) -> CallSearchResult? {
        let text = (content["outputTranscription"] as? [String: Any])?["text"] as? String
        let metadata = content["groundingMetadata"] as? [String: Any]
        let html = (metadata?["searchEntryPoint"] as? [String: Any])?["renderedContent"] as? String
        if text != nil || html != nil {
            if complete { self.text = ""; self.html = nil }
            complete = false
            if let text { self.text += text }
            if let html, !html.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { self.html = html }
            result = self.html.map { CallSearchResult(html: $0, text: self.text) }
        }
        if content["turnComplete"] as? Bool == true || content["interrupted"] as? Bool == true { complete = true }
        return result
    }
}
