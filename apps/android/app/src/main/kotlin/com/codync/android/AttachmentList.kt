package com.codync.android

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Files sent with a message, on the sender's side (iOS `AttachmentList`): images as pictures (tap for full size, hold for
 *  `menu` with Copy image and Copy name), other files as cards. */
@Composable internal fun AttachmentList(attachments: List<Attachment>, botId: String, menu: MessageMenu, state: AppState, store: AppStore) {
    var viewing by remember { mutableStateOf<Attachment?>(null) }
    Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        for (file in attachments) {
            // The tap is the picture's own button: the chat's tap-to-dismiss-keyboard takes a tap the menu's hold would leave.
            if (file.isImage) Box(Modifier.contextActions(reactions = menu.reactions) { menu.items(imageCopyItems(store, file.imageSource(botId, store))) }) {
                AttachmentImage(file, botId, 720, state, store, Modifier.sizeIn(maxWidth = 220.dp, maxHeight = 280.dp)
                    .clip(RoundedRectangle(16.dp)).plainButton { viewing = file }.semantics { contentDescription = file.name })
            } else AttachmentCard(file)
        }
    }
    AttachmentImageSheet(viewing, botId, menu, state, store) { viewing = null }
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

/** A picture from the computer the viewer and its menus act on (desktop `ImageSource`): its cache id, its name, and how to
 *  fetch its original file. */
internal class ImageSource(val id: String, val name: String, val load: suspend () -> java.io.File)

internal fun Attachment.imageSource(botId: String, store: AppStore) = ImageSource(id, name) { store.attachment(botId, this) }

internal fun SharedFile.imageSource(entryId: String, store: AppStore) = ImageSource(id, name) { store.sharedImage(entryId, this) }

/** A sent image, fetched from the computer once and then read from the cache; a placeholder until then. */
@Composable internal fun AttachmentImage(file: Attachment, botId: String, maxPixels: Int, state: AppState, store: AppStore, modifier: Modifier,
    scale: ContentScale = ContentScale.Fit) = RemoteImage(file.imageSource(botId, store), maxPixels, state, modifier, scale)

/** An image a bot shared, like `AttachmentImage`. */
@Composable internal fun SharedImage(file: SharedFile, entryId: String, maxPixels: Int, state: AppState, store: AppStore, modifier: Modifier) =
    RemoteImage(file.imageSource(entryId, store), maxPixels, state, modifier, ContentScale.Fit)

/** A picture from the computer (iOS `AttachmentImage`); a placeholder until it's here. */
@Composable private fun RemoteImage(source: ImageSource, maxPixels: Int, state: AppState, modifier: Modifier, scale: ContentScale) {
    val bitmap = rememberRemoteImage(source, maxPixels, state)
    if (bitmap != null) Image(bitmap, contentDescription = null, modifier = modifier, contentScale = scale) else ImagePlaceholder(modifier)
}

/** `source`'s picture, fetched from the computer into the cache and decoded to at most `maxPixels` a side; null until then. */
@Composable internal fun rememberRemoteImage(source: ImageSource, maxPixels: Int, state: AppState): ImageBitmap? {
    var image by remember(source.id, maxPixels) { mutableStateOf<ImageBitmap?>(null) }
    // Retried whenever the link changes: an image the computer couldn't send stays a placeholder, as on iOS.
    LaunchedEffect(source.id, maxPixels, state.actualConnection) {
        if (image != null) return@LaunchedEffect
        try {
            val file = source.load()
            image = withContext(Dispatchers.IO) { AttachmentFiles.thumbnail(file, maxPixels).asImageBitmap() }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { /* The placeholder stays; the next link change tries again. */ }
    }
    return image
}

/** Where a picture shows until it's here. */
@Composable internal fun ImagePlaceholder(modifier: Modifier = Modifier) {
    Box(modifier.size(160.dp, 120.dp).clip(RoundedRectangle(16.dp)).background(Palette.bubbleUser), contentAlignment = Alignment.Center) {
        Symbol("photo", size = 20.dp, tint = Palette.tertiary)
    }
}

/** The symbol for a file by its extension. */
internal object AttachmentIcon {
    fun symbol(name: String): String = when (name.substringAfterLast('.', "").lowercase()) {
        "png", "jpg", "jpeg", "heic", "gif", "webp", "tiff", "bmp" -> "photo"
        "pdf" -> "doc.richtext"
        "zip", "gz", "tar", "7z" -> "doc.zipper"
        "mov", "mp4", "m4v" -> "film"
        "mp3", "m4a", "wav", "aac" -> "waveform"
        else -> "doc"
    }
}
