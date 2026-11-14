package com.codync.android.core

import kotlinx.serialization.json.*

/** Google's original suggestions HTML and its answer, held only during the call. */
data class CallSearchResult(val html: String, val text: String)

/** Joins transcription and grounding metadata for the same model turn. Transcription is not ordered with the other
 * server messages, so a piece that arrives after `turnComplete` still belongs to that turn: only the next turn's
 * model content starts a new one. */
class CallSearchTurn {
    var result: CallSearchResult? = null
        private set
    /** Turns that came with search suggestions. */
    var answers = 0
        private set
    private var text = ""
    private var html: String? = null
    private var complete = false

    fun receive(content: JsonObject): CallSearchResult? {
        val text = (content["outputTranscription"] as? JsonObject)?.text("text")
        val metadata = content["groundingMetadata"] as? JsonObject
        val html = (metadata?.get("searchEntryPoint") as? JsonObject)?.text("renderedContent")
        val next = complete && "modelTurn" in content
        if (next) { this.text = ""; this.html = null; complete = false }
        if (text != null) this.text += text
        if (!html.isNullOrBlank()) {
            if (this.html == null) answers++
            this.html = html
        }
        if (next || text != null || html != null) result = this.html?.let { CallSearchResult(it, this.text) }
        if ((content["turnComplete"] as? JsonPrimitive)?.booleanOrNull == true || (content["interrupted"] as? JsonPrimitive)?.booleanOrNull == true) complete = true
        return result
    }
}
