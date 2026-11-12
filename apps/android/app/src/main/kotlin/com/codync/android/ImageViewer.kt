package com.codync.android

import androidx.compose.animation.Crossfade
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.LocalPinnableContainer
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.Attachment
import com.codync.android.core.Entry
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** A sent image full size, in a sheet while `file` is set. `menu` is its message's; the Library has none. */
@Composable internal fun AttachmentImageSheet(file: Attachment?, botId: String, menu: MessageMenu?, state: AppState, store: AppStore,
    dismiss: () -> Unit) {
    ImageSheet(file, { it.imageSource(botId, store) }, menu, state, store, dismiss)
}

/** An image a bot shared full size, in a sheet while `file` is set, with its card's Download button by the close button. */
@Composable internal fun SharedImageSheet(file: SharedFile?, entry: Entry, menu: MessageMenu, state: AppState, store: AppStore, dismiss: () -> Unit) {
    ImageSheet(file, { it.imageSource(entry.id, store) }, menu, state, store, dismiss,
        detail = { shown -> state.fileDownloads[shown.id]?.takeIf { it !is FileDownloadState.Saved }?.let { detail(shown, it) } }) { shown ->
        DownloadButton(entry, shown, state, store, inViewer = true)
    }
}

/**
 * A picture full size (iOS `AttachmentViewer`), in a sheet while `item` is set: it zooms, the header copies it, and holding
 * it opens its message's menu with Copy image and Copy name. `detail` is a line under the name (a shared file's download while it runs or failed); `trailing`, buttons
 * before the close button.
 */
@Composable private fun <T : Any> ImageSheet(item: T?, source: (T) -> ImageSource, menu: MessageMenu?, state: AppState, store: AppStore,
    dismiss: () -> Unit, detail: (T) -> String? = { null }, trailing: @Composable RowScope.(T) -> Unit = {}) {
    val open = item != null
    LaunchedEffect(open) { if (open) Analytics.capture(AnalyticsEvent.ImagePreviewOpened) }
    // The sheet lives in the chat row that opened it. A resize (split screen) can lay the chat out once without that row,
    // which would drop the sheet, so the row stays composed while the picture is open.
    val row = LocalPinnableContainer.current
    DisposableEffect(open, row) {
        val pinned = if (open) row?.pin() else null
        onDispose { pinned?.release() }
    }
    CodyncSheet(item, dismiss) { shown ->
        val picture = source(shown)
        val copy = rememberImageCopy(picture, store)
        // Reply and Show what it did leave the viewer, so it closes first; a reaction keeps it open.
        val items = { (menu?.items(copy.items(), leave = dismiss) ?: copy.items()) }
        // One zoom report per opening.
        var zoomReported by remember(open) { mutableStateOf(false) }
        Column(Modifier.fillMaxSize().background(Palette.background)) {
            ModalHeader({ ViewerTitle(picture.name, detail(shown)) }) {
                CopyImageButton(copy)
                trailing(shown)
            }
            val bitmap = rememberRemoteImage(picture, 4096, state)
            Box(Modifier.weight(1f).fillMaxWidth().contextActions(reactions = menu?.reactions, items = items)
                .semantics { contentDescription = picture.name }, contentAlignment = Alignment.Center) {
                if (bitmap == null) ImagePlaceholder()
                else ZoomableImage(bitmap, open, Modifier.fillMaxSize()) {
                    if (!zoomReported) { zoomReported = true; Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.Zoom, ImageFrom.Viewer)) }
                }
            }
        }
    }
}

/** The file's name as it is (never translated), and how its download goes. */
@Composable private fun ViewerTitle(name: String, detail: String?) {
    Column {
        Text(name, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
        if (detail != null) Text(detail, style = Type.caption2, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** How the viewer's last Copy image went, shown for a moment on its button. */
internal enum class CopyResult(val title: String, val symbol: String) {
    Copied("Copied", "checkmark"), Failed("Couldn't copy image", "exclamationmark.circle"),
}

/** The viewer's Copy image (desktop `useImageCopy`): copies the picture, then shows how it went for [FLASH_MS]. */
@Stable private class ImageCopy(private val source: ImageSource, private val store: AppStore, private val scope: CoroutineScope) {
    var result by mutableStateOf<CopyResult?>(null)
        private set
    private var flash: Job? = null

    fun copy() {
        flash?.cancel()
        flash = scope.launch {
            result = try {
                when (store.copyImage(source, ImageFrom.Viewer)) {
                    ImageCopyOutcome.Copied -> CopyResult.Copied
                    // A newer copy shows its own result.
                    ImageCopyOutcome.Overtaken -> return@launch
                }
            } catch (cancelled: CancellationException) { throw cancelled }
                catch (error: Exception) { android.util.Log.w("Codync", "Couldn't copy an image", error); CopyResult.Failed }
            delay(FLASH_MS)
            result = null
        }
    }

    /** Copy image and Copy name for the picture's menu; Copy image here also shows on the button. */
    fun items() = imageCopyItems(::copy) { store.copyImageName(source, ImageFrom.Viewer) }

    companion object {
        const val FLASH_MS = 1_500L
    }
}

@Composable private fun rememberImageCopy(source: ImageSource, store: AppStore): ImageCopy {
    val scope = rememberCoroutineScope()
    return remember(source.id) { ImageCopy(source, store, scope) }
}

@Composable private fun CopyImageButton(copy: ImageCopy) {
    Crossfade(copy.result, animationSpec = Motion.fade(), label = "Copy image") { result ->
        IconButton(result?.title ?: "Copy image", result?.symbol ?: "doc.on.doc", action = copy::copy)
    }
}
