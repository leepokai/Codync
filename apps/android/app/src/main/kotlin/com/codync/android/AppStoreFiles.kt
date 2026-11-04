package com.codync.android

import android.content.ClipData
import android.content.ClipboardManager
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*

// Files a bot shares in a chat (iOS `FileDownloads`): one download at a time, checked against its
// SHA-256, then handed to the system share sheet. Any picture in a chat also copies to the clipboard.

/** Where a shared file's download is, as iOS `FileDownloads.State`. */
sealed interface FileDownloadState {
    data class Downloading(val received: Long) : FileDownloadState
    data object Saved : FileDownloadState
    data class Failed(val message: String) : FileDownloadState
}

private fun downloadKey(fileId: String) = "file:$fileId"

/** Downloads `file` (shared in message `entryId` of chat `botId`) and opens the share sheet, if that chat is still on screen by then. */
fun AppStore.saveFile(entryId: String, botId: String, file: SharedFile) {
    val running = state.value.busy.filter { it.startsWith("file:") }
    if (running.isNotEmpty()) {
        if (downloadKey(file.id) !in running) setDownload(file.id, FileDownloadState.Failed("Finish the current download first."))
        return
    }
    action(downloadKey(file.id)) {
        try {
            setDownload(file.id, FileDownloadState.Downloading(0))
            val folder = withContext(Dispatchers.IO) { attachments.shareFolder() }
            val saved = HostFiles(ready()).downloadShared(entryId, file, folder) { setDownload(file.id, FileDownloadState.Downloading(it)) }
            setDownload(file.id, FileDownloadState.Saved)
            if (showing(botId)) attachments.open(saved, file.name)
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

/**
 * Picture `source` itself on the clipboard (desktop `copyImageOf`), its original file fetched first if it isn't cached yet.
 * It runs in the store's scope, so closing the viewer that asked doesn't stop it; the caller only waits for how it went.
 */
internal suspend fun AppStore.copyImage(source: ImageSource) = contextScope.async {
    val file = source.load()
    withContext(Dispatchers.IO) { attachments.copyImage(file, source.name) }
    Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.CopyImage))
}.await()

/** Copy image from a menu: a failure shows as the store's error. */
internal fun AppStore.copyImageLater(source: ImageSource) = action("copy:" + source.id) { copyImage(source) }

/** Just a picture's file name on the clipboard, as text. */
internal fun AppStore.copyImageName(source: ImageSource) {
    application.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Name", source.name))
    Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.CopyName))
}

/** The chat is the one open. */
private fun AppStore.showing(botId: String): Boolean = foreground && state.value.selectedBot == botId
