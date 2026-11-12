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

/**
 * Downloads `file` (shared in message `entryId` of chat `botId`) and opens the share sheet, if that chat is still on screen by
 * then. `ended` (iOS `FileDownloads.save`) hears once how the download ended, saved (`null`) or failed; never on a cancel.
 */
internal fun AppStore.saveFile(entryId: String, botId: String, file: SharedFile, ended: ((ImageError?) -> Unit)? = null) {
    val running = state.value.busy.filter { it.startsWith("file:") }
    if (running.isNotEmpty()) {
        if (downloadKey(file.id) !in running) {
            setDownload(file.id, FileDownloadState.Failed("Finish the current download first."))
            ended?.invoke(ImageError.Transfer)
        }
        return
    }
    action(downloadKey(file.id)) {
        val saved = try {
            setDownload(file.id, FileDownloadState.Downloading(0))
            val folder = withContext(Dispatchers.IO) { attachments.shareFolder() }
            HostFiles(ready()).downloadShared(entryId, file, folder) { setDownload(file.id, FileDownloadState.Downloading(it)) }
        } catch (cancelled: CancellationException) {
            setDownload(file.id, null)
            throw cancelled
        } catch (error: Exception) {
            setDownload(file.id, FileDownloadState.Failed(error.message ?: "Couldn't download the file."))
            ended?.invoke(ImageError.Transfer)
            return@action
        }
        setDownload(file.id, FileDownloadState.Saved)
        ended?.invoke(null)
        try {
            if (showing(botId)) attachments.open(saved, file.name)
        } catch (error: Exception) {
            setDownload(file.id, FileDownloadState.Failed(error.message ?: "Couldn't download the file."))
        }
    }
}

/** A file's one download control (iOS `FileDownloads.toggle`): downloads it, or cancels the download under way. */
fun AppStore.toggleFile(entryId: String, botId: String, file: SharedFile) =
    if (state.value.fileDownloads[file.id] is FileDownloadState.Downloading) cancelFile(file.id) else saveFile(entryId, botId, file)

fun AppStore.cancelFile(fileId: String) {
    actions[downloadKey(fileId)]?.cancel()
    setDownload(fileId, null)
}

private fun AppStore.setDownload(fileId: String, download: FileDownloadState?) = mutableState.update {
    it.copy(fileDownloads = if (download == null) it.fileDownloads - fileId else it.fileDownloads + (fileId to download))
}

/** The chat is the one open. */
private fun AppStore.showing(botId: String): Boolean = foreground && state.value.selectedBot == botId
