package com.codync.android

import android.content.ClipData
import android.content.ClipboardManager
import kotlinx.coroutines.*

// Any picture in a chat, sent or shared, on the clipboard as a picture or as its name (iOS `ImageCopy`).

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
    fun report(error: ImageError?) = Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.CopyImage, from, error))
    val clip = try {
        stagedImage(source)
    } catch (failure: ImageActionFailure) {
        if (!imageCopies.isLatest(request)) return@async ImageCopyOutcome.Overtaken
        report(failure.kind)
        throw failure
    }
    if (!imageCopies.isLatest(request)) return@async ImageCopyOutcome.Overtaken
    try {
        attachments.putOnClipboard(clip)
    } catch (error: Exception) {
        report(ImageError.Clipboard)
        throw ImageActionFailure(ImageError.Clipboard, COPY_FAILED, error)
    }
    report(null)
    ImageCopyOutcome.Copied
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

/** Just picture `source`'s file name on the clipboard, as text, overtaking a picture copy still converting so that one can't land over it. */
internal fun AppStore.copyImageName(source: ImageSource, from: ImageFrom) {
    imageCopies.start()
    application.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Name", source.name))
    Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.CopyName, from))
}
