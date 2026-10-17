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

fun AppStore.editDraft(botId: String, thread: String?, text: String) {
    if (state.value.loading) return
    val lane = ComposerDraft.lane(botId, thread)
    val current = state.value.drafts[lane]
    if (current?.text == text || current == null && text.isEmpty()) return
    try { updateDraft((current ?: ComposerDraft(botId, thread)).copy(text = text)) }
    catch (error: Exception) { showError(error) }
}

fun AppStore.addDraftFiles(botId: String, thread: String?, files: List<OutgoingFile>) {
    check(!state.value.loading) { "Wait for the account to finish opening before adding files." }
    val current = state.value.drafts[ComposerDraft.lane(botId, thread)] ?: ComposerDraft(botId, thread)
    updateDraft(current.copy(files = current.files + files))
}

fun AppStore.removeDraftFile(botId: String, thread: String?, file: OutgoingFile) = action("draft-file:" + file.path) {
    val current = state.value.drafts[ComposerDraft.lane(botId, thread)] ?: return@action
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

fun AppStore.sendDraft(botId: String, thread: String?, text: String, onQueued: () -> Unit) {
    editDraft(botId, thread, text)
    val lane = ComposerDraft.lane(botId, thread)
    val draft = state.value.drafts[lane] ?: return
    if (draft.text != text) return
    if (draft.text.isBlank() && draft.files.isEmpty()) return
    action("draft-send:" + lane) {
        // Persist its nonce before the pending commit. Recovery removes a draft
        // already represented by a pending or confirmed send with this nonce.
        persistDrafts()
        draftWriter?.join()
        check(savedDrafts[lane] == draft) { "Couldn't save this draft. Retry to send it." }
        val send = PendingSend(draft.nonce, botId, thread, draft.text.trim(), System.currentTimeMillis(), files = draft.files)
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

suspend fun AppStore.attachment(botId: String, file: Attachment): File {
    val request = contextScope.async { downloadLocks.getOrPut(file.id) { Mutex() }.withLock {
    val destination = withContext(Dispatchers.IO) { attachments.cached(file.id) }
    if (!destination.exists() || destination.length() != file.size) {
        val active = ready()
        HostFiles(active).download(botId, file, destination)
        withContext(Dispatchers.IO) { attachments.trim() }
    }
    destination
    } }
    try { return request.await() } finally { request.cancel() }
}
