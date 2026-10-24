package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*

// Voice calls, run by the computer (host/src/chat/calls, iOS `CallLink`): the voice's instructions, tools and
// transcript live there; the engines only carry audio. What a call sends the bot, and the bot's replies while it
// lasts, carry the call's id; the chat shows them only in the call's conversation, which its line opens.

/** What a client needs to run a call's voice, from the computer (`startCall`). */
data class CallSession(val credential: String?, val instructions: String, val tools: List<CallTool>) {
    companion object {
        fun decode(value: JsonObject) = CallSession(
            credential = value.text("credential"),
            instructions = value.text("instructions") ?: throw IllegalStateException("The computer sent no call instructions."),
            tools = (value["tools"] as? JsonArray).orEmpty().mapNotNull { tool ->
                val fields = tool as? JsonObject ?: return@mapNotNull null
                CallTool(fields.text("name") ?: return@mapNotNull null, fields.text("description").orEmpty(),
                    fields["parameters"] as? JsonObject ?: buildJsonObject { put("type", "object"); putJsonObject("properties") { } })
            },
        )
    }
}

/** A call as the computer runs it; one per call, so it outlives an engine switch. */
interface CallLink {
    /** Starts the call on the computer (again, when a provider session is replaced): what the voice needs. */
    suspend fun start(provider: VoiceProvider, model: String, voice: String): CallSession
    /** One of the voice's tool calls, run on the computer. A failure is the answer. */
    suspend fun tool(name: String, arguments: JsonObject): JsonElement
    /** A piece of what `from` (`user` or `voice`) said; the pieces of one turn join into one line. */
    fun add(from: String, piece: String)
    /** The user stopped talking and their words are being written down (`true`), or that failed (`false`): a tool
     * call waits a moment for them. */
    fun transcribing(on: Boolean)
    /** The turn is over: what was said goes to the computer. */
    fun endTurn()
    /** How a final reply of the bot reaches the voice. */
    fun reply(text: String): String
    fun notice(text: String): String = "[Notice] $text"
}

/** One function the voice can call, in JSON Schema form; each provider spells it its own way. */
data class CallTool(val name: String, val description: String, val parameters: JsonObject) {
    fun openAI(): JsonObject = buildJsonObject {
        put("type", "function"); put("name", name); put("description", description); put("parameters", parameters)
    }
    fun gemini(): JsonObject = buildJsonObject {
        put("name", name); put("description", description)
        if (parameters["properties"]?.jsonObject?.isNotEmpty() == true) put("parameters", geminiSchema(parameters))
    }
    /** Gemini's schema spells types in capitals (`OBJECT`, `STRING`). */
    private fun geminiSchema(schema: JsonObject): JsonObject = JsonObject(schema.mapValues { (key, value) ->
        when {
            key == "type" && value is JsonPrimitive -> JsonPrimitive(value.content.uppercase())
            key == "properties" && value is JsonObject -> JsonObject(value.mapValues { (_, it) -> (it as? JsonObject)?.let(::geminiSchema) ?: JsonObject(emptyMap()) })
            else -> value
        }
    })
}

/** One line of a call: what the user said (`user`) or the voice answered (`voice`), with when (ms). */
@Serializable data class CallLine(val from: String, val text: String, val at: Long? = null)

/**
 * What was said on a realtime call, the pieces of a turn joined, until it goes to the computer ([drain]); [turnEnded]
 * says a turn is over.
 */
class CallTranscript(private val clock: () -> Long = System::currentTimeMillis, private val script: (String) -> String = { it },
    private val turnEnded: () -> Unit) {
    private val lines = mutableListOf<CallLine>()
    private var open = false

    fun add(from: String, piece: String) {
        if (piece.isEmpty()) return
        val last = lines.lastOrNull()
        if (open && last?.from == from) lines[lines.lastIndex] = last.copy(text = last.text + piece)
        else { endTurn(); lines += CallLine(from, piece, clock()); open = true }
    }

    fun endTurn() { open = false; turnEnded() }

    /** The lines finished so far ([withOpen]: and the one still being said), out of the buffer: trimmed and through
     * [script] (Chinese in the reader's script). */
    fun drain(withOpen: Boolean): List<CallLine> {
        val count = if (open && !withOpen) lines.size - 1 else lines.size
        if (withOpen) open = false
        val taken = lines.take(count.coerceAtLeast(0))
        repeat(taken.size) { lines.removeAt(0) }
        return taken.mapNotNull { line -> script(line.text.trim()).takeIf { it.isNotEmpty() }?.let { line.copy(text = it) } }
    }

    /** Lines the computer didn't get, for the next request. */
    fun restore(said: List<CallLine>) { lines.addAll(0, said) }
}

/** One message of a call's conversation. */
data class CallMessage(val speaker: Speaker, val text: String) {
    enum class Speaker { User, Voice, VoiceToBot, Bot }
}

/** The voice call it was said in (user), answered during (agent), or that it ends (notice). */
val Entry.call: String? get() = data.text("call")

/** Without what voice calls carried: the messages of a call that has its line, or of a call this phone is on. */
fun List<Entry>.hidingCalls(live: Set<String>): List<Entry> {
    val ended = filter { it.kind == "notice" }.mapNotNull { it.call }.toSet()
    return filter { it.kind == "notice" || it.call?.let { call -> call !in ended && call !in live } ?: true }
}

/** A call's conversation, oldest first: what was said (the voice's transcript) and the messages it carried. */
fun List<Entry>.callConversation(line: Entry): List<CallMessage> {
    val call = line.call ?: return emptyList()
    val said = (line.data["transcript"] as? JsonArray).orEmpty().mapNotNull { element ->
        val fields = element as? JsonObject ?: return@mapNotNull null
        val speaker = if (fields.text("from") == "user") CallMessage.Speaker.User else CallMessage.Speaker.Voice
        ((fields["at"] as? JsonPrimitive)?.longOrNull ?: line.createdAt) to CallMessage(speaker, fields.text("text").orEmpty())
    } + filter { it.call == call && (it.kind == "user" || it.kind == "agent" && it.data["final"]?.jsonPrimitive?.booleanOrNull == true) }.map { e ->
        val speaker = when {
            e.kind == "agent" -> CallMessage.Speaker.Bot
            e.data.text("from") == "voice" -> CallMessage.Speaker.VoiceToBot
            else -> CallMessage.Speaker.User
        }
        e.createdAt to CallMessage(speaker, e.text)
    }
    // Stable: phone and computer clocks are merged as they are.
    return said.sortedBy { it.first }.map { it.second }
}
