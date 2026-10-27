package com.codync.android.core

import kotlinx.serialization.json.*

/** Google's original suggestions HTML and its answer, held only during the call. */
data class CallSearchResult(val html: String, val text: String)

/** Joins metadata and transcription that arrive separately during the same model turn. */
class CallSearchTurn {
    var result: CallSearchResult? = null
        private set
    private var text = ""
    private var html: String? = null
    private var complete = true

    fun receive(content: JsonObject): CallSearchResult? {
        val text = (content["outputTranscription"] as? JsonObject)?.text("text")
        val metadata = content["groundingMetadata"] as? JsonObject
        val html = (metadata?.get("searchEntryPoint") as? JsonObject)?.text("renderedContent")
        if (text != null || html != null) {
            if (complete) { this.text = ""; this.html = null }
            complete = false
            if (text != null) this.text += text
            if (!html.isNullOrBlank()) this.html = html
            result = this.html?.let { CallSearchResult(it, this.text) }
        }
        if ((content["turnComplete"] as? JsonPrimitive)?.booleanOrNull == true || (content["interrupted"] as? JsonPrimitive)?.booleanOrNull == true) complete = true
        return result
    }
}
