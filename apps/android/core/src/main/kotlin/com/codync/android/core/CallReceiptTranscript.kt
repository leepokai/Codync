package com.codync.android.core

import kotlinx.serialization.json.*

/** Bounded text observed during a call; no claim that audio played or decisions were approved. */
class CallReceiptTranscript {
    private data class Segment(val role: String, val source: String, val text: String)
    private val rows = linkedMapOf<String, Segment>()
    private var characters = 0
    private var sequence = 0
    private var geminiTurn = 0
    private var truncated = false
    private var interruptions = 0

    @Synchronized fun add(role: String, source: String, text: String, id: String? = null, delta: Boolean = false) {
        if (text.isEmpty() || (id?.length ?: 0) > 256) return
        val captured = text.replace("\u0000", "")
        if (captured.length != text.length) truncated = true
        if (captured.isEmpty()) return
        val key = id ?: "local-${sequence++}"
        val old = rows[key]
        if (old == null && rows.size >= 96) { truncated = true; return }
        if (old != null && (old.role != role || old.source != source)) return
        val available = minOf(4000, 32000 - characters + (old?.text?.length ?: 0))
        val full = (if (delta) old?.text.orEmpty() else "") + captured
        var next = full.take(available)
        if (next.lastOrNull()?.isHighSurrogate() == true) next = next.dropLast(1)
        if (next.length < full.length) truncated = true
        if (next.isBlank()) return
        characters += next.length - (old?.text?.length ?: 0)
        rows[key] = Segment(role, source, next)
    }

    @Synchronized fun interrupted() { interruptions = minOf(10000, interruptions + 1) }

    fun openAI(event: JsonObject) {
        val item = (event["item_id"] as? JsonPrimitive)?.contentOrNull ?: return
        val index = (event["content_index"] as? JsonPrimitive)?.intOrNull ?: return
        val text = (event["transcript"] as? JsonPrimitive)?.contentOrNull ?: return
        val id = "openai-$item-$index"
        when ((event["type"] as? JsonPrimitive)?.contentOrNull) {
            "conversation.item.input_audio_transcription.completed" -> add("user", "recognized", text, id)
            "response.output_audio_transcript.done" -> add("assistant", "generated", text, id)
        }
    }

    @Synchronized fun gemini(content: JsonObject) {
        for ((field, role) in listOf("inputTranscription" to "user", "outputTranscription" to "assistant")) {
            val text = ((content[field] as? JsonObject)?.get("text") as? JsonPrimitive)?.contentOrNull ?: continue
            add(role, if (role == "user") "recognized" else "generated", text, "gemini-$geminiTurn-$role", delta = true)
        }
        if ((content["interrupted"] as? JsonPrimitive)?.booleanOrNull == true) interrupted()
        if ((content["turnComplete"] as? JsonPrimitive)?.booleanOrNull == true) geminiTurn++
    }

    @Synchronized fun snapshot(): JsonObject = buildJsonObject {
        put("truncated", truncated); put("interruptions", interruptions)
        putJsonArray("entries") {
            for (entry in rows.values) addJsonObject {
                put("role", entry.role); put("source", entry.source); put("text", entry.text)
            }
        }
    }
}
