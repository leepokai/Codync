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

/**
 * Downloads `file` (shared in message `entryId` of chat `botId`) and opens the share sheet, if that chat is still on screen by
 * then. A download from the image viewer (`inViewer`) is reported when it ends, saved or failed; a cancel reports nothing.
 */
fun AppStore.saveFile(entryId: String, botId: String, file: SharedFile, inViewer: Boolean = false) {
    val running = state.value.busy.filter { it.startsWith("file:") }
    if (running.isNotEmpty()) {
        if (downloadKey(file.id) !in running) {
            setDownload(file.id, FileDownloadState.Failed("Finish the current download first."))
            if (inViewer) Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.Download, ImageFrom.Viewer, ImageError.Transfer))
        }
        return
    }
    action(downloadKey(file.id)) {
        var ended = false
        fun report(error: ImageError?) {
            ended = true
            if (inViewer) Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.Download, ImageFrom.Viewer, error))
        }
        try {
            setDownload(file.id, FileDownloadState.Downloading(0))
            val folder = withContext(Dispatchers.IO) { attachments.shareFolder() }
            val saved = HostFiles(ready()).downloadShared(entryId, file, folder) { setDownload(file.id, FileDownloadState.Downloading(it)) }
            setDownload(file.id, FileDownloadState.Saved)
            report(null)
            if (showing(botId)) attachments.open(saved, file.name)
        } catch (cancelled: CancellationException) {
            setDownload(file.id, null)
            throw cancelled
        } catch (error: Exception) {
            if (!ended) report(ImageError.Transfer)
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

/** A picture action that failed, and why (the analytics `error`); its message is what the user sees. */
internal class ImageActionFailure(val kind: ImageError, message: String, cause: Throwable? = null) : Exception(message, cause)

/** Numbers each copy as it starts (iOS `CopyRequests`), so one that finishes after a newer one began knows it was overtaken. */
internal class CopyRequests {
    private var latest = 0

    fun start(): Int = ++latest

    fun isLatest(request: Int): Boolean = request == latest
}

/** How a copy of the picture itself went, short of failing (which throws [ImageActionFailure]). */
internal enum class ImageCopyOutcome {
    Copied,

    /** A newer copy started before this one finished: the clipboard and showing how it went are that one's, and this one reports nothing. */
    Overtaken,
}

/**
 * Picture `source` itself on the clipboard (desktop `copyImageOf`), its original file fetched first if it isn't cached yet,
 * reported as done or failed `from` where it was asked. It runs in the store's scope, so closing the viewer that asked
 * doesn't stop it; the caller only waits for how it went.
 */
internal suspend fun AppStore.copyImage(source: ImageSource, from: ImageFrom): ImageCopyOutcome = contextScope.async {
    val request = imageCopies.start()
    val staged = try { Result.success(stagedImage(source)) } catch (failure: ImageActionFailure) { Result.failure(failure) }
    if (!imageCopies.isLatest(request)) return@async ImageCopyOutcome.Overtaken
    try {
        val clip = staged.getOrThrow()
        try { attachments.putOnClipboard(clip) } catch (error: Exception) { throw ImageActionFailure(ImageError.Clipboard, COPY_FAILED, error) }
        Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.CopyImage, from))
        ImageCopyOutcome.Copied
    } catch (failure: ImageActionFailure) {
        Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.CopyImage, from, failure.kind))
        throw failure
    }
}.await()

/** Picture `source` as a clip, its original file fetched first if it isn't cached yet. */
private suspend fun AppStore.stagedImage(source: ImageSource): ClipData {
    val file = try { source.load() }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { throw ImageActionFailure(ImageError.Unavailable, error.message ?: COPY_FAILED, error) }
    return try { withContext(Dispatchers.IO) { attachments.stageImage(file, source.name) } }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { throw ImageActionFailure(ImageError.Convert, COPY_FAILED, error) }
}

private const val COPY_FAILED = "Couldn't copy image"

/** Copy image from a picture's menu while no viewer is open: a failure shows in the app's error dialog. */
internal fun AppStore.copyImageLater(source: ImageSource) = action("copy:" + source.id) { copyImage(source, ImageFrom.Menu) }

/** Just a picture's file name on the clipboard, as text. */
internal fun AppStore.copyImageName(source: ImageSource, from: ImageFrom) {
    application.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Name", source.name))
    Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.CopyName, from))
}

/** The chat is the one open. */
private fun AppStore.showing(botId: String): Boolean = foreground && state.value.selectedBot == botId
