package com.codync.android

import android.util.AtomicFile
import com.codync.android.core.MAX_FILE_BYTES
import com.codync.android.core.MirrorSnapshot
import com.codync.android.core.OutgoingFile
import com.codync.android.core.WireJson
import java.io.File
import java.util.UUID
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject

@Serializable
data class ComposerDraft(val botId: String, val text: String = "",
    val files: List<OutgoingFile> = emptyList(), val nonce: String = UUID.randomUUID().toString())

/** One bounded draft book per account/computer; a draft's nonce also identifies its durable send. */
class DraftBook(private val directory: File, computerId: String) {
    private val file = bookFile(directory, computerId)

    @Synchronized fun read(): List<ComposerDraft> {
        if (!file.baseFile.exists()) return emptyList()
        require(file.baseFile.length() <= MAX_BYTES) { "The saved drafts are too large." }
        // A book saved before 2.20.1 may hold thread drafts (a `threadId`): only each chat's own stays.
        val drafts = WireJson.parseToJsonElement(file.readFully().toString(Charsets.UTF_8)).jsonArray
            .filter { (it.jsonObject["threadId"] ?: JsonNull) == JsonNull }
            .map { WireJson.decodeFromJsonElement<ComposerDraft>(it) }
        validate(drafts)
        return drafts
    }

    /** A crash between pending-send commit and draft removal cannot offer the same draft twice. */
    @Synchronized fun recover(snapshot: MirrorSnapshot): List<ComposerDraft> {
        val queued = snapshot.pending.map { it.nonce }.toSet() + snapshot.entries.mapNotNull { it.clientNonce }
        val drafts = read().filter { it.nonce !in queued }
        save(drafts)
        return drafts
    }

    @Synchronized fun save(drafts: Collection<ComposerDraft>) {
        validate(drafts)
        if (drafts.isEmpty()) { file.delete(); return }
        val bytes = WireJson.encodeToString(drafts.toList()).toByteArray()
        require(bytes.size <= MAX_BYTES) { "The saved drafts are too large." }
        val output = file.startWrite()
        try { output.write(bytes); file.finishWrite(output) }
        catch (error: Throwable) { file.failWrite(output); throw error }
    }

    fun validate(drafts: Collection<ComposerDraft>, checkPaths: Boolean = true) {
        require(drafts.size <= 64) { "Too many drafts. Send or clear an older draft first." }
        require(drafts.map { it.botId }.distinct().size == drafts.size) { "The saved drafts contain duplicate conversations." }
        val outgoing = if (checkPaths) File(directory, "outgoing").canonicalFile else null
        for (draft in drafts) {
            require(draft.botId.isNotBlank() && draft.botId.length <= 128)
            require(UUID.fromString(draft.nonce).toString() == draft.nonce) { "A saved draft has an invalid message ID." }
            require(draft.text.toByteArray().size <= TEXT_BYTES) { "Draft messages must be 64 KiB or smaller." }
            require(draft.files.size <= 20) { "Choose at most 20 files for one message." }
            for (attachment in draft.files) {
                require(attachment.size in 0..MAX_FILE_BYTES && attachment.name.isNotBlank())
                if (checkPaths) require(File(attachment.path).canonicalFile.parentFile == outgoing) { "A draft attachment belongs to another account." }
            }
        }
    }

    companion object {
        private const val MAX_BYTES = 8 * 1024 * 1024
        /** The most UTF-8 a draft's message holds. */
        const val TEXT_BYTES = 64 * 1024
        private fun bookFile(directory: File, computerId: String): AtomicFile {
            require(computerId.matches(Regex("[A-Za-z0-9_-]{22}")))
            return AtomicFile(File(directory, "drafts-$computerId.json"))
        }
        /** Deletes a computer's drafts. */
        fun delete(directory: File, computerId: String) { bookFile(directory, computerId).delete() }
    }
}
