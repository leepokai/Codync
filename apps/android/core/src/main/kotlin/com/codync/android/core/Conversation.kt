package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

val WireJson = Json { ignoreUnknownKeys = true; encodeDefaults = true }

@Serializable
data class Entry(
    val id: String,
    val seq: Long,
    val botId: String,
    val rev: Long,
    val kind: String,
    val turn: Long = 0,
    val data: JsonObject = JsonObject(emptyMap()),
    val createdAt: Long = 0,
    val updatedAt: Long = 0,
) {
    val text: String get() = data.text("text").orEmpty()
    val clientNonce: String? get() = data.text("clientNonce")
    val status: String? get() = data.text("status")
    /** Files the bot shared in this message. */
    val files: List<SharedFile> get() = data["files"]?.let { WireJson.decodeFromJsonElement<List<SharedFile>>(it) }.orEmpty()
    /** The artifact a bot published in this message (an artifact card), read leniently: its text stands in for it. */
    val artifact: ArtifactCard? get() = (data["artifact"] as? JsonObject)?.let { runCatching { WireJson.decodeFromJsonElement<ArtifactCard>(it) }.getOrNull() }
    /** Of memory and context, only a fresh conversation gets a line: memory lives in the bot's memory settings, a
     *  summary keeps the same conversation going, and a kept handoff changed nothing. */
    val isChat: Boolean get() = kind in listOf("user", "permission") ||
        kind == "notice" && "memory" !in data && "memoryFile" !in data && "compaction" !in data &&
            (data["handoff"] as? JsonObject)?.text("status") != "kept" ||
        kind == "agent" && data["final"]?.jsonPrimitive?.booleanOrNull == true
    /** A step the working indicator unfolds: thinking, narration, a tool call or a plan; never the bot's bookkeeping,
     *  nor a call the host didn't flag (one older than 2.25.0 flags none). */
    val isWorkingStep: Boolean get() = when (kind) {
        "thought" -> text.isNotBlank()
        "agent" -> data["final"]?.jsonPrimitive?.booleanOrNull != true && !isPass(text)
        "tool" -> data["bookkeeping"]?.jsonPrimitive?.booleanOrNull == false
        "plan" -> true
        else -> false
    }
    /** A message the user can quote in a reply: theirs, or a final reply (a group member's too). */
    val isReplyable: Boolean get() = kind == "user" || kind == "agent" && data["final"]?.jsonPrimitive?.booleanOrNull == true
    /** The message this one replies to, as the computer quoted it. */
    val replyTo: ReplyQuote? get() = (data["replyTo"] as? JsonObject)?.let { runCatching { WireJson.decodeFromJsonElement<ReplyQuote>(it) }.getOrNull() }
    /** What the composer shows of the message it replies to: its text, or the names of its files. */
    val quotePreview: String get() = text.ifEmpty {
        (files.map { it.name } + (data["attachments"] as? JsonArray).orEmpty().mapNotNull { (it as? JsonObject)?.text("name") }).joinToString(", ")
    }
    /** How a reply quotes this message until the computer's own quote arrives. */
    val replyQuote: ReplyQuote get() = ReplyQuote(id, if (kind == "user") "user" else data.text("author") ?: botId, quotePreview.take(500))

    companion object {
        /** No text, or a pass the host keeps out of the chat (`is_pass` in host/src/chat/group.rs). */
        private fun isPass(text: String) =
            text.trim().trim('"', '.', '*').lowercase().let { it.isEmpty() || it == "(pass)" || it == "pass" }

        fun decode(value: JsonObject): Entry {
            val entry = WireJson.decodeFromJsonElement<Entry>(value)
            require(entry.id.isNotBlank() && entry.botId.isNotBlank() && entry.kind.isNotBlank() && entry.seq >= 0 && entry.rev >= 0) {
                "The computer sent an invalid conversation update."
            }
            // Validate the fields used by rendering before the mirror can checkpoint them.
            val fields = WireJson.decodeFromJsonElement<EntryFields>(entry.data)
            require(fields.callSeconds == null || fields.callSeconds >= 0)
            require(fields.options.orEmpty().all { it.optionId.isNotBlank() })
            require(fields.attachments.orEmpty().all { it.size in 0..MAX_FILE_BYTES && it.name.isNotBlank() })
            return entry
        }
    }
}

const val MAX_FILE_BYTES = 100L * 1024 * 1024

@Serializable
private data class EntryFields(
    val text: String? = null, val clientNonce: String? = null, val status: String? = null,
    val final: Boolean? = null, val title: String? = null, val command: String? = null,
    val detail: String? = null, val output: String? = null, val author: String? = null,
    val options: List<PermissionChoice>? = null, val reactions: List<String>? = null,
    val attachments: List<Attachment>? = null, val files: List<SharedFile>? = null,
    val callSeconds: Long? = null,
)
@Serializable private data class PermissionChoice(val optionId: String, val name: String, val kind: String)
/** A quoted message: its entry, who wrote it (`user` or a bot id) and its text, cut short by the computer; `artifact` when it
 *  quotes an artifact card (the message is about that artifact). */
@Serializable data class ReplyQuote(val id: String = "", val author: String = "", val text: String = "", val artifact: String? = null)

@Serializable data class Attachment(val id: String, val name: String, val size: Long) {
    val isImage: Boolean get() = name.substringAfterLast('.', "").lowercase() in listOf("png", "jpg", "jpeg", "heic", "gif", "webp", "tiff", "bmp")
}

/** A file a bot shared: immutable on the computer, checked against its SHA-256 once downloaded. */
@Serializable data class SharedFile(val id: String, val name: String, val size: Long, val sha256: String) {
    /** Before any of it is read: a name that can't leave its folder and a hash to check. */
    fun validate() {
        require(size in 0..MAX_FILE_BYTES && sha256.length == 64 && sha256.all { it in "0123456789abcdef" } &&
            name.isNotEmpty() && name.toByteArray(Charsets.UTF_8).size <= 200 && name != "." && name != ".." &&
            '/' !in name && '\\' !in name && name.codePoints().noneMatch { Character.getType(it).toByte() in CONTROLS }) { "Invalid file metadata." }
    }
}

/** Unicode control and format characters, which a file name may not contain. */
private val CONTROLS = setOf(Character.CONTROL, Character.FORMAT)

@Serializable
enum class SendStatus { Sending, Waiting, Delivering, Failed }

@Serializable
data class PendingSend(
    val nonce: String,
    val botId: String,
    val text: String,
    val createdAt: Long,
    val status: SendStatus = SendStatus.Sending,
    val error: String? = null,
    val files: List<OutgoingFile> = emptyList(),
    /** The message it quotes (`send`'s `replyTo` is its id), shown on the bubble until the computer has it. */
    val replyTo: ReplyQuote? = null,
) {
    /** The `send` request for it, with the ids of its uploaded files. */
    fun body(attachments: List<String> = emptyList()): JsonObject = buildJsonObject {
        put("botId", botId); put("text", text); put("clientNonce", nonce)
        replyTo?.let { put("replyTo", it.id) }
        if (attachments.isNotEmpty()) put("attachments", JsonArray(attachments.map(::JsonPrimitive)))
    }
}

@Serializable
data class OutgoingFile(val path: String, val name: String, val size: Long)

sealed interface MirrorEvent {
    data class Hello(val hostId: String, val rev: Long, val usage: JsonObject, val screen: JsonObject?) : MirrorEvent
    data class BotChanged(val bot: Bot) : MirrorEvent
    data class EntryChanged(val entry: Entry) : MirrorEvent
    data class Usage(val value: JsonObject) : MirrorEvent
    data class Screen(val value: JsonObject) : MirrorEvent
    /** An artifact was published, updated or renamed: live only, never a checkpoint. */
    data class ArtifactChanged(val artifact: Artifact) : MirrorEvent
    data object Resync : MirrorEvent
    data object Ignored : MirrorEvent

    companion object {
        fun decode(value: JsonObject): MirrorEvent = when (value.text("type")) {
            "hello" -> Hello(requireNotNull(value.text("hostId")),
                requireNotNull(value["rev"]?.jsonPrimitive?.longOrNull),
                value["usage"]?.jsonObject ?: JsonObject(emptyMap()), value["screen"]?.takeUnless { it == JsonNull }?.jsonObject)
            "bot" -> BotChanged(HostClient.decode(value.getValue("bot").jsonObject))
            "entry" -> EntryChanged(Entry.decode(value.getValue("entry").jsonObject))
            "usage" -> Usage(value.getValue("usage").jsonObject)
            "screen" -> Screen(value.getValue("screen").jsonObject)
            // Not in the catch-up: one this app can't read is passed over rather than fetching everything again.
            "artifact" -> runCatching { ArtifactChanged(Artifact.decode(value.getValue("artifact"))) }.getOrDefault(Ignored)
            "resync" -> Resync
            // Match the existing clients: unknown events do not advance the cursor.
            else -> Ignored
        }
    }
}

data class MirrorSnapshot(
    val bots: List<Bot> = emptyList(),
    val entries: List<Entry> = emptyList(),
    val pending: List<PendingSend> = emptyList(),
    val rev: Long = 0,
    val usage: JsonObject = JsonObject(emptyMap()),
    val screen: JsonObject? = null,
)
