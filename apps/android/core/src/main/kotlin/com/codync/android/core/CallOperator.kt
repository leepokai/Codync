package com.codync.android.core

import kotlinx.serialization.json.*

/** What a call's operator works against: the app's mirror and its send/approval actions. */
interface CallTarget {
    val bots: List<Bot>
    val entries: List<Entry>
    fun send(text: String)
    fun respond(entry: Entry, optionId: String)
}

/** One function the realtime model can call, in JSON Schema form (both providers take the same shape). */
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

/**
 * A realtime model is an operator in front of the bot, not a replacement for it (iOS `CallOperator`):
 * the bot does the work, the operator talks. Its tools run here against the app; only `send_to_bot`
 * messages and the bot's replies become chat entries.
 */
class CallOperator(private val botId: String, private val target: CallTarget) {
    private val bot: Bot? get() = target.bots.firstOrNull { it.id == botId }

    val instructions: String get() {
        val bot = bot
        val name = bot?.name ?: "the bot"
        val about = if (bot == null || bot.description.isEmpty()) "" else " (${bot.description})"
        val who = if (bot?.kind == "group") {
            val members = target.bots.filter { it.id in bot.members }.joinToString(", ") { it.name }
            "the group chat $name$about on the user's computer, where the bots $members answer in turn " +
                "(a message that @-mentions one of them asks just that bot). The bots do all the actual work; you relay"
        } else "$name$about, a coding agent running on the user's computer. $name does all the actual work; you relay"
        return """
            You are the voice on a phone call between the user and $who.
            - When the user asks for something to be done, changed, checked or answered about their code or computer, call send_to_bot with their request in their own words (keep details, names and code terms exactly). Don't do the work or invent results yourself.
            - Use bot_status and recent_messages for "what are you doing?" or "what did it say?".
            - Call answer_approval only after the user has clearly said which option to pick.
            - Messages starting with [$name replied] or [Notice] come from the computer: tell the user what they say, shortened for speech (no code blocks, no long lists, no URLs).
            - Reply in the user's language. Keep spoken answers short and natural.
        """.trimIndent()
    }

    /** How a reply is handed to the operator. */
    fun reply(text: String): String = "[${bot?.name ?: "Bot"} replied] ${SpokenText.from(text)}"
    fun notice(text: String): String = "[Notice] $text"

    /** Runs one tool call; the result goes back to the model as JSON. */
    fun call(name: String, arguments: JsonObject): JsonObject = when (name) {
        "send_to_bot" -> send(arguments.text("text").orEmpty())
        "bot_status" -> status()
        "recent_messages" -> recent(arguments["count"]?.jsonPrimitive?.let { it.intOrNull ?: it.doubleOrNull?.toInt() } ?: 6)
        "answer_approval" -> answer(arguments.text("option").orEmpty())
        else -> error("Unknown tool $name.")
    }

    private fun send(text: String): JsonObject {
        val message = text.trim()
        if (message.isEmpty()) return error("Nothing to send.")
        target.send(message)
        return buildJsonObject { put("sent", true); put("note", "The bot's reply will arrive as a separate message.") }
    }

    private fun status(): JsonObject {
        val bot = bot ?: return error("The bot isn't available.")
        return buildJsonObject {
            put("status", bot.status); put("activity", bot.activity)
            pendingApproval?.let { card ->
                putJsonObject("approval") {
                    put("request", card.data.text("title") ?: card.data.text("command") ?: "")
                    put("options", JsonArray(options(card).map { JsonPrimitive(it.second) }))
                }
            }
        }
    }

    private fun recent(count: Int): JsonObject {
        val name = bot?.name ?: "bot"
        val messages = chat.filter(Entry::isChat).takeLast(count.coerceIn(1, 20)).map { entry ->
            val from = when (entry.kind) {
                "user" -> "user"
                "agent" -> entry.data.text("author")?.let { author -> target.bots.firstOrNull { it.id == author }?.name } ?: name
                "permission" -> "approval request"
                else -> "notice"
            }
            buildJsonObject { put("from", from); put("text", SpokenText.from(entry.data.text("text") ?: entry.data.text("title") ?: "")) }
        }
        return buildJsonObject { put("messages", JsonArray(messages)) }
    }

    private fun answer(option: String): JsonObject {
        val card = pendingApproval ?: return error("Nothing is waiting for approval.")
        val options = options(card)
        val wanted = option.lowercase()
        val pick = options.firstOrNull { it.second.lowercase() == wanted || it.first == option }
            ?: options.firstOrNull { it.second.lowercase().contains(wanted) || wanted.contains(it.second.lowercase()) }
            ?: return buildJsonObject { put("error", "No such option."); put("options", JsonArray(options.map { JsonPrimitive(it.second) })) }
        target.respond(card, pick.first)
        return buildJsonObject { put("answered", pick.second) }
    }

    private val chat: List<Entry> get() = target.entries.filter { it.botId == botId }.sortedBy(Entry::seq)
    private val pendingApproval: Entry? get() = chat.lastOrNull { it.kind == "permission" && it.status == "pending" }

    /** A permission card's options: id → name. */
    private fun options(card: Entry): List<Pair<String, String>> = (card.data["options"] as? JsonArray).orEmpty().mapNotNull { option ->
        val fields = option as? JsonObject ?: return@mapNotNull null
        val id = fields.text("optionId") ?: return@mapNotNull null
        id to (fields.text("name") ?: id)
    }

    private fun error(message: String) = buildJsonObject { put("error", message) }

    companion object {
        val tools = listOf(
            CallTool("send_to_bot", "Send a message to the bot on the computer, exactly as if the user typed it. Its reply arrives later.",
                schema { putJsonObject("text") { put("type", "string"); put("description", "The message") } }.withRequired("text")),
            CallTool("bot_status", "Whether the bot is idle or working, what it is doing, and any approval it is waiting for.", schema { }),
            CallTool("recent_messages", "The latest messages in the chat with the bot, oldest first.",
                schema { putJsonObject("count") { put("type", "integer"); put("description", "How many, 1–20") } }),
            CallTool("answer_approval", "Answer the bot's pending approval request with one of its options.",
                schema { putJsonObject("option") { put("type", "string"); put("description", "The option's name") } }.withRequired("option")),
        )
        private fun schema(properties: JsonObjectBuilder.() -> Unit) = buildJsonObject { put("type", "object"); putJsonObject("properties", properties) }
        private fun JsonObject.withRequired(name: String) = JsonObject(this + ("required" to JsonArray(listOf(JsonPrimitive(name)))))
    }
}
