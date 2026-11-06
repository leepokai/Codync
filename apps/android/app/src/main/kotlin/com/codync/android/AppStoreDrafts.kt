package com.codync.android

import android.net.Uri
import java.io.File
import java.nio.ByteBuffer
import java.nio.CharBuffer
import java.nio.charset.CodingErrorAction
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Composer drafts (saved as you type) and the files that go with them.

fun AppStore.editDraft(botId: String, text: String) {
    if (state.value.loading) return
    val current = state.value.drafts[botId]
    if (current?.text == text || current == null && text.isEmpty()) return
    try { updateDraft((current ?: ComposerDraft(botId)).copy(text = text)) }
    catch (error: Exception) { showError(error) }
}

/** Puts [text] after what's in [botId]'s message box (text shared from another app), as much of it as the box holds. */
fun AppStore.appendDraft(botId: String, text: String) = editDraft(botId, sharedDraft(state.value.drafts[botId]?.text.orEmpty(), text))

/** [shared] after [before], cut at a character's edge to the most a draft holds (`DraftBook.TEXT_BYTES` of UTF-8). */
internal fun sharedDraft(before: String, shared: String): String {
    val joined = DictationText.join(before, shared)
    val input = CharBuffer.wrap(joined)
    // The encoder stops before the first character that doesn't fit, never inside one; a lone surrogate counts as the '?' it's saved as.
    Charsets.UTF_8.newEncoder().onMalformedInput(CodingErrorAction.REPLACE).encode(input, ByteBuffer.allocate(DraftBook.TEXT_BYTES), true)
    return joined.substring(0, input.position())
}

fun AppStore.addDraftFiles(botId: String, files: List<OutgoingFile>) {
    check(!state.value.loading) { "Wait for the account to finish opening before adding files." }
    val current = state.value.drafts[botId] ?: ComposerDraft(botId)
    updateDraft(current.copy(files = current.files + files))
}

fun AppStore.removeDraftFile(botId: String, file: OutgoingFile) = action("draft-file:" + file.path) {
    val current = state.value.drafts[botId] ?: return@action
    updateDraft(current.copy(files = current.files - file))
    draftWriter?.join()
    check(savedDrafts.values.none { draft -> draft.files.any { it.path == file.path } }) { "Couldn't save the attachment removal. Retry before leaving this chat." }
    if (state.value.pending.none { send -> send.files.any { it.path == file.path } }) withContext(Dispatchers.IO) { attachments.discard(file) }
}

private fun AppStore.updateDraft(draft: ComposerDraft) {
    val book = draftBook ?: return
    val drafts = if (draft.text.isEmpty() && draft.files.isEmpty()) state.value.drafts - draft.botId else state.value.drafts + (draft.botId to draft)
    book.validate(drafts.values, checkPaths = false)
    mutableState.update { it.copy(drafts = drafts) }
    persistDrafts()
}

internal fun AppStore.persistDrafts() {
    if (draftWriter?.isActive == true) return
    val book = draftBook ?: return
    draftWriter = contextScope.launch {
        try {
            while (isActive) {
                val saving = state.value.drafts.values.toList()
                withContext(Dispatchers.IO) { book.save(saving) }
                savedDrafts = saving.associateBy { it.botId }
                if (state.value.drafts.values.toList() == saving) break
            }
        } catch (error: CancellationException) { throw error }
        catch (_: Exception) { mutableState.update { it.copy(error = "Couldn't save this draft. Keep the app open and retry.") } }
        finally { draftWriter = null }
    }
}

/** Sends draft `key` (the chat's, or one of its artifacts') to chat `botId`, quoting `replyTo` when there is one. */
fun AppStore.sendDraft(key: String, botId: String, text: String, replyTo: ReplyQuote?, onQueued: () -> Unit) {
    editDraft(key, text)
    val draft = state.value.drafts[key] ?: return
    if (draft.text != text) return
    if (draft.text.isBlank() && draft.files.isEmpty()) return
    action("draft-send:$key") {
        // Persist its nonce before the pending commit. Recovery removes a draft
        // already represented by a pending or confirmed send with this nonce.
        persistDrafts()
        draftWriter?.join()
        check(savedDrafts[key] == draft) { "Couldn't save this draft. Retry to send it." }
        val send = PendingSend(draft.nonce, botId, draft.text.trim(), System.currentTimeMillis(), files = draft.files, replyTo = replyTo)
        savePending(send)
        mutableState.update { it.copy(drafts = it.drafts - key) }
        persistDrafts()
        onQueued()
        action("send:" + send.nonce) { deliver(send) }
    }
}

fun AppStore.importFiles(uris: List<Uri>, onImported: (List<OutgoingFile>) -> Unit) = action("attachments") {
    val imported = mutableListOf<OutgoingFile>()
    try {
        for (uri in uris) {
            require(uri.scheme == "content") { "Choose a file using Photos or Files." }
            imported.add(attachments.import(uri))
        }
        onImported(imported.toList())
    } catch (error: Throwable) {
        withContext(NonCancellable + Dispatchers.IO) { imported.forEach(attachments::discard) }
        throw error
    }
}
fun AppStore.removeFile(file: OutgoingFile) = action("remove:" + file.path) { withContext(Dispatchers.IO) { attachments.discard(file) } }

suspend fun AppStore.attachment(botId: String, file: Attachment): File =
    cachedFile(file.id, file.size) { files, destination -> files.download(botId, file, destination) }

/** A picture a bot shared (iOS `sharedFileData`): checked against its SHA-256 on its way into the same cache. */
suspend fun AppStore.sharedImage(entryId: String, file: SharedFile): File = cachedFile(file.id, file.size) { files, destination ->
    val folder = withContext(Dispatchers.IO) { attachments.shareFolder() }
    val saved = files.downloadShared(entryId, file, folder)
    withContext(Dispatchers.IO) { try { saved.copyTo(destination, overwrite = true) } finally { folder.deleteRecursively() } }
}

/** A file from the computer, fetched once into the attachment cache by `fetch`. */
private suspend fun AppStore.cachedFile(id: String, size: Long, fetch: suspend (HostFiles, File) -> Unit): File {
    val request = contextScope.async { downloadLocks.getOrPut(id) { Mutex() }.withLock {
    val destination = withContext(Dispatchers.IO) { attachments.cached(id) }
    if (!destination.exists() || destination.length() != size) {
        fetch(HostFiles(ready()), destination)
        withContext(Dispatchers.IO) { attachments.trim() }
    }
    destination
    } }
    try { return request.await() } finally { request.cancel() }
}
