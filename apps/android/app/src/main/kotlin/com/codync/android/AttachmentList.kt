package com.codync.android

import android.webkit.WebView
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import coil3.compose.AsyncImage
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import java.io.File
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/**
 * Files sent with a message, on the sender's side (iOS `AttachmentPreview`): pictures and clips as a
 * stack with a full-size viewer, other files as cards.
 */
@Composable internal fun AttachmentList(attachments: List<Attachment>, botId: String, state: AppState, store: AppStore) {
    val media = attachments.filter { it.isImage || it.isVideo }
    val files = attachments.filterNot { it.isImage || it.isVideo }
    var viewing by remember { mutableStateOf<Int?>(null) }
    Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (media.isNotEmpty()) MediaMosaic(media.map { MediaItem(it.id, it.name, it.isVideo) }, { viewing = it }) { item, modifier ->
            val file = media.first { it.id == item.key }
            if (file.isVideo) VideoTile(file.name, modifier)
            else AttachmentImage(file, botId, 720, state, store, modifier, ContentScale.Crop)
        }
        for (file in files) AttachmentCard(file)
    }
    MediaViewerSheet(media.takeIf { viewing != null }, viewing ?: 0, { it.name }, { viewing = null }) { file, modifier, playing ->
        if (file.isVideo) VideoPlayer({ store.attachment(botId, file) }, playing, modifier)
        else AttachmentImage(file, botId, 4096, state, store, modifier)
    }
}

/** Files a bot shared: pictures and clips as a stack with a full-size viewer (save included), other files as cards. */
@Composable internal fun SharedFilesList(entry: Entry, files: List<SharedFile>, state: AppState, store: AppStore) {
    val media = files.filter { (it.isImage || it.isVideo) && it.size <= 20L * 1024 * 1024 }
    val rest = files.filterNot { it in media }
    var viewing by remember { mutableStateOf<Int?>(null) }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (media.isNotEmpty()) MediaMosaic(media.map { MediaItem(it.id, it.name, it.isVideo) }, { viewing = it }) { item, modifier ->
            val file = media.first { it.id == item.key }
            if (file.isVideo) VideoTile(file.name, modifier)
            else SharedImage(file, entry.id, 720, state, store, modifier, ContentScale.Crop)
        }
        for (file in rest) FileDownloadCard(entry, file, state, store)
    }
    MediaViewerSheet(media.takeIf { viewing != null }, viewing ?: 0, { it.name }, { viewing = null },
        trailing = { file -> IconButton("Save " + file.name, "arrow.down.to.line") { store.saveFile(entry.id, entry.botId, file) } }) { file, modifier, playing ->
        if (file.isVideo) VideoPlayer({ store.sharedImage(entry.id, file) }, playing, modifier)
        else SharedImage(file, entry.id, 4096, state, store, modifier)
    }
}

/** A sent image full size, in a sheet while `file` is set. */
@Composable internal fun AttachmentImageSheet(file: Attachment?, botId: String, state: AppState, store: AppStore, dismiss: () -> Unit) {
    ImageSheet(file, { it.name }, dismiss) { shown, modifier -> AttachmentImage(shown, botId, 4096, state, store, modifier) }
}

/** An image a bot shared full size, in a sheet while `file` is set. */
@Composable internal fun SharedImageSheet(file: SharedFile?, entryId: String, state: AppState, store: AppStore, dismiss: () -> Unit) {
    ImageSheet(file, { it.name }, dismiss) { shown, modifier -> SharedImage(shown, entryId, 4096, state, store, modifier) }
}

/** A picture full size (iOS `AttachmentViewer`), in a sheet while `item` is set. */
@Composable private fun <T : Any> ImageSheet(item: T?, name: (T) -> String, dismiss: () -> Unit, image: @Composable (T, Modifier) -> Unit) {
    CodyncSheet(item, dismiss) { shown ->
        Column(Modifier.fillMaxSize().background(Palette.background)) {
            ModalHeader(name(shown))
            Box(Modifier.weight(1f).padding(12.dp), contentAlignment = Alignment.Center) { image(shown, Modifier.fillMaxSize()) }
        }
    }
}

@Composable private fun AttachmentCard(file: Attachment) {
    Row(Modifier.clip(RoundedRectangle(16.dp)).background(Palette.bubbleUser).semantics(mergeDescendants = true) { }
        .padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Symbol(AttachmentIcon.symbol(file.name), size = 19.dp, tint = Palette.secondary)
        Column(verticalArrangement = Arrangement.spacedBy(1.dp)) {
            Text(file.name, style = Type.subheadline, color = Palette.text, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
            Text(formatBytes(file.size), style = Type.caption2, color = Palette.tertiary)
        }
    }
}

/** A sent image, fetched from the computer once and then read from the cache; a placeholder until then. */
@Composable internal fun AttachmentImage(file: Attachment, botId: String, maxPixels: Int, state: AppState, store: AppStore, modifier: Modifier,
    scale: ContentScale = ContentScale.Fit) =
    RemoteImage(file.id, file.name.isSvgName, maxPixels, state, modifier, scale) { store.attachment(botId, file) }

/** An image a bot shared, like `AttachmentImage`. */
@Composable internal fun SharedImage(file: SharedFile, entryId: String, maxPixels: Int, state: AppState, store: AppStore, modifier: Modifier,
    scale: ContentScale = ContentScale.Fit) =
    RemoteImage(file.id, file.isSvg, maxPixels, state, modifier, scale) { store.sharedImage(entryId, file) }

internal val String.isSvgName: Boolean get() = substringAfterLast('.', "").equals("svg", ignoreCase = true)

/**
 * A picture `load` fetches from the computer into the cache (iOS `AttachmentImage`); a placeholder
 * until then. Raster pictures are decoded at no more than `maxPixels`; an SVG is drawn by its text.
 */
@Composable private fun RemoteImage(id: String, svg: Boolean, maxPixels: Int, state: AppState, modifier: Modifier, scale: ContentScale, load: suspend () -> File) {
    var image by remember(id, maxPixels) { mutableStateOf<ImageBitmap?>(null) }
    var vector by remember(id) { mutableStateOf<File?>(null) }
    // Retried whenever the link changes: an image the computer couldn't send stays a placeholder, as on iOS.
    LaunchedEffect(id, maxPixels, state.actualConnection) {
        if (image != null || vector != null) return@LaunchedEffect
        try {
            val source = load()
            if (svg) vector = source else image = withContext(Dispatchers.IO) { AttachmentFiles.thumbnail(source, maxPixels).asImageBitmap() }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { /* The placeholder stays; the next link change tries again. */ }
    }
    val bitmap = image
    val svgFile = vector
    when {
        bitmap != null -> Image(bitmap, contentDescription = null, modifier = modifier, contentScale = scale)
        svgFile != null -> AsyncImage(svgFile, contentDescription = null, imageLoader = LocalNetworkImages.current,
            modifier = modifier, contentScale = scale)
        else -> Box(modifier.size(160.dp, 120.dp).clip(RoundedRectangle(16.dp)).background(Palette.bubbleUser), contentAlignment = Alignment.Center) {
            Symbol("photo", size = 20.dp, tint = Palette.tertiary)
        }
    }
}

/** The symbol for a file by its extension. */
internal object AttachmentIcon {
    fun symbol(name: String): String = when (name.substringAfterLast('.', "").lowercase()) {
        "png", "jpg", "jpeg", "heic", "gif", "webp", "tiff", "bmp", "svg" -> "photo"
        "pdf" -> "doc.richtext"
        "zip", "gz", "tar", "7z" -> "doc.zipper"
        "mov", "mp4", "m4v", "webm" -> "film"
        "mp3", "m4a", "wav", "aac" -> "waveform"
        else -> "doc"
    }
}
