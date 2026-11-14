package com.codync.android

import android.net.Uri
import java.io.File
import java.util.UUID
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
        lastSentNonce = send.nonce
        savePending(send)
        mutableState.update { it.copy(drafts = it.drafts - key) }
        persistDrafts()
        onQueued()
        action("send:" + send.nonce) { deliver(send) }
    }
}

/**
 * [text] and [files] go to chat [botId] as one message, as its box sends one ([sendDraft]) but leaving its draft alone:
 * something another app shared, [pasted] what of it came from that app. [ShareOutcome.Queued] when the computer can't take it
 * now and its relay or this phone holds it.
 */
internal suspend fun AppStore.sendShared(botId: String, text: String, files: List<OutgoingFile>, pasted: List<String>): ShareOutcome {
    val send = PendingSend(UUID.randomUUID().toString(), botId, text.trim(), System.currentTimeMillis(), files = files, pasted = pasted)
    // Once saved, the message and its files are the chat's: the share sheet closing mid-way mustn't drop either.
    withContext(NonCancellable) {
        discardOnFailure(files) { withContext(Dispatchers.IO) { requireNotNull(database).save(send) } }
        lastSentNonce = send.nonce
        action("send:" + send.nonce) { deliver(send) }
        // In the chat before it opens.
        refresh()
    }
    return if (state.value.actualConnection is LinkState.Ready) ShareOutcome.Sent else ShareOutcome.Queued
}

fun AppStore.importFiles(uris: List<Uri>, onImported: (List<OutgoingFile>) -> Unit) = action("attachments") {
    val files = importAll(uris)
    discardOnFailure(files) { onImported(files) }
}

/** [uris] copied into this account's outgoing files; none stay when one of them fails. */
internal suspend fun AppStore.importAll(uris: List<Uri>): List<OutgoingFile> {
    val imported = mutableListOf<OutgoingFile>()
    discardOnFailure(imported) {
        for (uri in uris) {
            require(uri.scheme == "content") { "Choose a file using Photos or Files." }
            imported.add(attachments.import(uri))
        }
    }
    return imported.toList()
}

/** Runs [use]; when it fails, [files] are deleted again. */
private suspend fun <T> AppStore.discardOnFailure(files: List<OutgoingFile>, use: suspend () -> T): T =
    try { use() } catch (error: Throwable) {
        withContext(NonCancellable + Dispatchers.IO) { files.forEach(attachments::discard) }
        throw error
    }
fun AppStore.removeFile(file: OutgoingFile) = action("remove:" + file.path) { withContext(Dispatchers.IO) { attachments.discard(file) } }

suspend fun AppStore.attachment(botId: String, file: Attachment): File =
    cachedFile(file.id, file.size) { files, destination -> files.download(botId, file, destination) }

/** A picture or sound a bot shared (iOS `sharedFileData`): checked against its SHA-256 on its way into the same cache. */
suspend fun AppStore.sharedFile(entryId: String, file: SharedFile): File = cachedFile(file.id, file.size) { files, destination ->
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
