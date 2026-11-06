package com.codync.android

import java.io.File
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import androidx.lifecycle.viewModelScope

// Files a bot shares in a chat (iOS `FileDownloads`): one download at a time, checked against its
// SHA-256, then handed to the system share sheet in the conversation it came from.

/** Where a shared file's download is, as iOS `FileDownloads.State`. */
sealed interface FileDownloadState {
    data class Downloading(val received: Long) : FileDownloadState
    data object Saved : FileDownloadState
    data class Failed(val message: String) : FileDownloadState
}

/** A verified download waiting for its share sheet (iOS `FileDownloads.Export`): it opens once its conversation is on screen. */
data class FileExport(val id: String, val path: String, val name: String, val entryId: String, val botId: String, val threadId: String?)

private fun downloadKey(fileId: String) = "file:$fileId"

/** Downloads `file`, then opens the share sheet in its conversation, now or when the reader comes back to it. */
fun AppStore.saveFile(entry: Entry, file: SharedFile) {
    val running = state.value.busy.filter { it.startsWith("file:") }
    if (running.isNotEmpty()) {
        if (downloadKey(file.id) !in running) setDownload(file.id, FileDownloadState.Failed("Finish the current download first."))
        return
    }
    // This file's share sheet is already waiting in its chat.
    if (state.value.export?.id == file.id) { presentExport(); return }
    // Another file's share sheet never showed because its chat was left; this tap replaces it.
    dropExport()
    action(downloadKey(file.id)) {
        try {
            setDownload(file.id, FileDownloadState.Downloading(0))
            val folder = withContext(Dispatchers.IO) { attachments.shareFolder() }
            val saved = HostFiles(ready()).downloadShared(entry.id, file, folder) { setDownload(file.id, FileDownloadState.Downloading(it)) }
            setDownload(file.id, FileDownloadState.Saved)
            // `observeExports` opens it now if its conversation is on screen, or once the reader is back there.
            mutableState.update { it.copy(export = FileExport(file.id, saved.path, file.name, entry.id, entry.botId, entry.threadId)) }
        } catch (cancelled: CancellationException) {
            setDownload(file.id, null)
            throw cancelled
        } catch (error: Exception) {
            setDownload(file.id, FileDownloadState.Failed(error.message ?: "Couldn't download the file."))
        }
    }
}

fun AppStore.cancelFile(fileId: String) {
    actions[downloadKey(fileId)]?.cancel()
    setDownload(fileId, null)
}

/** Opens the waiting share sheet when its conversation is the one on screen. */
internal fun AppStore.presentExport() {
    val export = state.value.export ?: return
    if (!showing(export)) return
    mutableState.update { it.copy(export = null) }
    val saved = File(export.path)
    // Shared copies are cleared after a day: an old one is downloaded again from the card.
    if (saved.exists()) attachments.open(saved, export.name) else setDownload(export.id, null)
}

/** Opens a waiting share sheet as soon as the reader is back in its conversation with Codync in front. */
internal fun AppStore.observeExports() {
    contextScope.launch {
        combine(state, foregroundState) { current, front -> Triple(current.export?.id, current.selectedBot to current.thread, front) }
            .distinctUntilChanged().collect { presentExport() }
    }
}

/** Deletes a share sheet that never showed (its chat wasn't opened again) with its downloaded copy. */
internal fun AppStore.dropExport() {
    val export = state.value.export ?: return
    mutableState.update { it.copy(export = null) }
    // Not the account's scope: a retiring account cancels that before the copy is gone.
    viewModelScope.launch(Dispatchers.IO) { File(export.path).parentFile?.deleteRecursively() }
}

private fun AppStore.setDownload(fileId: String, download: FileDownloadState?) = mutableState.update {
    it.copy(fileDownloads = if (download == null) it.fileDownloads - fileId else it.fileDownloads + (fileId to download))
}

/** The chat or thread the message is in (a thread's first message too) is the one open. */
private fun AppStore.showing(export: FileExport): Boolean {
    val current = state.value
    return foreground && current.selectedBot == export.botId && (current.thread == export.threadId || current.thread == export.entryId)
}

/** Library downloads retain the same deferred share ownership as conversation cards. */
fun AppStore.saveFile(entryId: String, botId: String, file: SharedFile) {
    val entry = state.value.entries.firstOrNull { it.id == entryId && it.botId == botId }
        ?: Entry(entryId, 0, botId, 0, "agent", 0, kotlinx.serialization.json.JsonObject(emptyMap()))
    saveFile(entry, file)
}
