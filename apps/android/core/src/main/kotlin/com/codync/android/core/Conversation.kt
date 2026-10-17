package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonNull
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
    val threadId: String? = null,
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
    val isChat: Boolean get() = kind in listOf("user", "permission", "notice") ||
        kind == "agent" && data["final"]?.jsonPrimitive?.booleanOrNull == true

    companion object {
        fun decode(value: JsonObject): Entry {
            val entry = WireJson.decodeFromJsonElement<Entry>(value)
            require(entry.id.isNotBlank() && entry.botId.isNotBlank() && entry.kind.isNotBlank() && entry.seq >= 0 && entry.rev >= 0) {
                "The computer sent an invalid conversation update."
            }
            // Validate the fields used by rendering before the mirror can checkpoint them.
            val fields = WireJson.decodeFromJsonElement<EntryFields>(entry.data)
            require(fields.thread == null || fields.thread.count >= 0)
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
    val thread: ReplySummary? = null, val attachments: List<Attachment>? = null, val files: List<SharedFile>? = null,
    val callSeconds: Long? = null,
)
@Serializable private data class PermissionChoice(val optionId: String, val name: String, val kind: String)
@Serializable private data class ReplySummary(val count: Int)
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

    private companion object { val CONTROLS = setOf(Character.CONTROL, Character.FORMAT) }
}

@Serializable
enum class SendStatus { Sending, Waiting, Delivering, Failed }

@Serializable
data class PendingSend(
    val nonce: String,
    val botId: String,
    val threadId: String? = null,
    val text: String,
    val createdAt: Long,
    val status: SendStatus = SendStatus.Sending,
    val error: String? = null,
    val files: List<OutgoingFile> = emptyList(),
)

@Serializable
data class OutgoingFile(val path: String, val name: String, val size: Long)

sealed interface MirrorEvent {
    data class Hello(val hostId: String, val rev: Long, val usage: JsonObject, val screen: JsonObject?) : MirrorEvent
    data class BotChanged(val bot: Bot) : MirrorEvent
    data class EntryChanged(val entry: Entry) : MirrorEvent
    data class Usage(val value: JsonObject) : MirrorEvent
    data class Screen(val value: JsonObject) : MirrorEvent
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
