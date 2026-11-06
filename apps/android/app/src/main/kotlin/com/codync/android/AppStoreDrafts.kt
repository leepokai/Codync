package com.codync.android

import android.net.Uri
import java.io.File
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Composer drafts (saved as you type) and the files that go with them.

fun AppStore.editDraft(botId: String, thread: String?, text: String, draftKey: String = ComposerDraft.lane(botId, thread)) {
    if (state.value.loading) return
    val lane = draftKey
    val current = state.value.drafts[lane]
    if (current?.text == text || current == null && text.isEmpty()) return
    try { updateDraft((current ?: ComposerDraft(botId, thread, draftKey = draftKey.takeUnless { it == ComposerDraft.lane(botId, thread) })).copy(text = text)) }
    catch (error: Exception) { showError(error) }
}

/** Quotes a whole message into the draft of the conversation it belongs to (idea 34's fallback). */
fun AppStore.quoteMessage(entry: Entry, thread: String? = entry.threadId) {
    val text = entry.text
    if (text.isBlank()) return
    val quoted = text.lineSequence().joinToString("\n") { "> $it" }
    val lane = ComposerDraft.lane(entry.botId, thread)
    val current = state.value.drafts[lane]?.text.orEmpty()
    editDraft(entry.botId, thread, if (current.isEmpty()) "$quoted\n\n" else "$current\n$quoted\n\n")
}

fun AppStore.addDraftFiles(botId: String, thread: String?, files: List<OutgoingFile>, draftKey: String = ComposerDraft.lane(botId, thread)) {
    check(!state.value.loading) { "Wait for the account to finish opening before adding files." }
    val current = state.value.drafts[draftKey] ?: ComposerDraft(botId, thread, draftKey = draftKey.takeUnless { it == ComposerDraft.lane(botId, thread) })
    updateDraft(current.copy(files = current.files + files))
}

/** A shared text and files at once: the draft is validated before either is applied, so a failure changes nothing. */
internal fun AppStore.addShareToDraft(botId: String, text: String, files: List<OutgoingFile>) {
    check(!state.value.loading) { "Wait for the account to finish opening before adding the shared content." }
    checkNotNull(draftBook) { "Connect to a computer to share this." }
    val current = state.value.drafts[ComposerDraft.lane(botId, null)] ?: ComposerDraft(botId)
    updateDraft(current.copy(
        text = if (text.isBlank()) current.text else ShareIntake.merge(current.text, text),
        files = current.files + files,
    ))
}

fun AppStore.removeDraftFile(botId: String, thread: String?, file: OutgoingFile, draftKey: String = ComposerDraft.lane(botId, thread)) = action("draft-file:" + file.path) {
    val current = state.value.drafts[draftKey] ?: return@action
    updateDraft(current.copy(files = current.files - file))
    draftWriter?.join()
    check(savedDrafts.values.none { draft -> draft.files.any { it.path == file.path } }) { "Couldn't save the attachment removal. Retry before leaving this chat." }
    if (state.value.pending.none { send -> send.files.any { it.path == file.path } }) withContext(Dispatchers.IO) { attachments.discard(file) }
}

private fun AppStore.updateDraft(draft: ComposerDraft) {
    val book = draftBook ?: return
    val drafts = if (draft.text.isEmpty() && draft.files.isEmpty()) state.value.drafts - draft.lane else state.value.drafts + (draft.lane to draft)
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
                savedDrafts = saving.associateBy { it.lane }
                if (state.value.drafts.values.toList() == saving) break
            }
        } catch (error: CancellationException) { throw error }
        catch (_: Exception) { mutableState.update { it.copy(error = "Couldn't save this draft. Keep the app open and retry.") } }
        finally { draftWriter = null }
    }
}

fun AppStore.sendDraft(botId: String, thread: String?, text: String, replyTo: ReplyQuote? = null, draftKey: String = ComposerDraft.lane(botId, thread), onQueued: () -> Unit) {
    editDraft(botId, thread, text, draftKey)
    val lane = draftKey
    val draft = state.value.drafts[lane] ?: return
    if (draft.text != text) return
    if (draft.text.isBlank() && draft.files.isEmpty()) return
    action("draft-send:" + lane) {
        // Persist its nonce before the pending commit. Recovery removes a draft
        // already represented by a pending or confirmed send with this nonce.
        persistDrafts()
        draftWriter?.join()
        check(savedDrafts[lane] == draft) { "Couldn't save this draft. Retry to send it." }
        val send = PendingSend(draft.nonce, botId, draft.text.trim(), System.currentTimeMillis(), files = draft.files,
            replyTo = replyTo, threadId = thread)
        savePending(send)
        mutableState.update { it.copy(drafts = it.drafts - lane) }
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

fun AppStore.editDraft(botId: String, text: String) = editDraft(botId, null, text)
