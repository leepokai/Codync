package com.codync.android

import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*

// Files a bot shares in a chat (iOS `FileDownloads`): one download at a time, checked against its
// SHA-256, then handed to the system share sheet.

/** Where a shared file's download is, as iOS `FileDownloads.State`. */
sealed interface FileDownloadState {
    data class Downloading(val received: Long) : FileDownloadState
    data object Saved : FileDownloadState
    data class Failed(val message: String) : FileDownloadState
}

private fun downloadKey(fileId: String) = "file:$fileId"

/** Downloads `file` and opens the share sheet, if its conversation is still on screen by then. */
fun AppStore.saveFile(entry: Entry, file: SharedFile) {
    val running = state.value.busy.filter { it.startsWith("file:") }
    if (running.isNotEmpty()) {
        if (downloadKey(file.id) !in running) setDownload(file.id, FileDownloadState.Failed("Finish the current download first."))
        return
    }
    action(downloadKey(file.id)) {
        try {
            setDownload(file.id, FileDownloadState.Downloading(0))
            val folder = withContext(Dispatchers.IO) { attachments.shareFolder() }
            val saved = HostFiles(ready()).downloadShared(entry.id, file, folder) { setDownload(file.id, FileDownloadState.Downloading(it)) }
            setDownload(file.id, FileDownloadState.Saved)
            if (showing(entry)) attachments.open(saved, file.name)
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

private fun AppStore.setDownload(fileId: String, download: FileDownloadState?) = mutableState.update {
    it.copy(fileDownloads = if (download == null) it.fileDownloads - fileId else it.fileDownloads + (fileId to download))
}

/** The chat or thread the message is in (a thread's first message too) is the one open. */
private fun AppStore.showing(entry: Entry): Boolean {
    val current = state.value
    return foreground && current.selectedBot == entry.botId && (current.thread == entry.threadId || current.thread == entry.id)
}
