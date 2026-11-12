package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.requiredHeight
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsPropertyReceiver
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlin.math.roundToInt
import kotlin.math.sign
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** A file a bot shared, as iOS `FileDownloadCard`: an image shows itself (tap for full size), a voice note plays, then its
 *  name and size, and a button to download (or cancel). Holding it opens its message's `menu`; an image's copies the
 *  picture or its name. */
@Composable internal fun FileDownloadCard(entry: Entry, file: SharedFile, menu: MessageMenu, state: AppState, store: AppStore) {
    val context = LocalContext.current
    val focus = LocalFocusManager.current
    var viewing by remember { mutableStateOf(false) }
    val player = if (file.showsAsAudio) rememberSharedAudio(entry, file, store) else null
    val scope = rememberCoroutineScope()
    val translate = translator()
    val caption = detail(file, state.fileDownloads[file.id], player, translate)
    val play = { player?.let { scope.launch { it.toggle() } }; Unit }
    Column(Modifier.fillMaxWidth().contextActions(reactions = menu.reactions, onClick = { focus.clearFocus() }) {
        menu.items(if (file.showsAsImage) imageCopyItems(store, file.imageSource(entry.id, store)) else listOf(textCopyItem(context, entry.text)))
    }.clip(RoundedRectangle(16.dp)).background(Palette.bubbleAgent).padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (file.showsAsImage) SharedImage(file, entry.id, 720, state, store, Modifier.sizeIn(maxHeight = 280.dp)
            .clip(RoundedRectangle(10.dp)).plainButton { viewing = true }.semantics { contentDescription = file.name })
        FileRow(entry, file, player, caption, play, state, store)
    }
    SharedImageSheet(file.takeIf { viewing }, entry, menu, state, store) { viewing = false }
}

@Composable private fun FileRow(entry: Entry, file: SharedFile, audio: SharedAudio?, caption: String, play: () -> Unit,
    state: AppState, store: AppStore) {
    val translate = translator()
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (audio != null) SharedAudioButton(audio, play, Modifier.semantics(mergeDescendants = true) {
            contentDescription = translate(if (audio.phase == AudioPhase.Playing) "Pause" else "Play")
            audioSeek(audio)
        }) else Symbol(AttachmentIcon.symbol(file.name), size = 24.dp, tint = Palette.secondary)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(file.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(caption, style = Type.caption2, color = if (audio?.phase == AudioPhase.Failed) Palette.danger else Palette.secondary)
            if (audio != null && audio.duration > 0.0) AudioProgress(audio)
        }
        DownloadButton(entry, file, state, store)
    }
}

/** A shared file's download button, on its card and in its viewer: downloads (again after a failure or a save), or cancels
 *  the download under way. In the image viewer (`inViewer`) its end is reported. */
@Composable internal fun DownloadButton(entry: Entry, file: SharedFile, state: AppState, store: AppStore, inViewer: Boolean = false) {
    val busy = state.fileDownloads[file.id] is FileDownloadState.Downloading
    Box(Modifier.size(36.dp).plainButton { if (busy) store.cancelFile(file.id) else store.saveFile(entry.id, entry.botId, file, if (inViewer) ::reportViewerDownload else null) }
        .semantics { contentDescription = (if (busy) "Cancel download " else "Download ") + file.name }, contentAlignment = Alignment.Center) {
        Symbol(if (busy) "xmark" else "arrow.down.to.line", size = 20.dp, tint = Palette.text)
    }
}

/** How a download from the image viewer ended. */
private fun reportViewerDownload(error: ImageError?) = Analytics.capture(AnalyticsEvent.ImagePreviewAction(ImageAction.Download, ImageFrom.Viewer, error))

/** The progress bar of a playing voice note, filled left to right, read again about four times a second while it
 *  plays; as wide as [modifier] lets it, the whole row by default. Tap or drag it to move there: its 48 dp touch strip
 *  overflows the 4 dp bar without taking up layout, as the play button's does. */
@Composable internal fun AudioProgress(audio: SharedAudio, modifier: Modifier = Modifier.fillMaxWidth()) {
    LaunchedEffect(audio, audio.phase) {
        audio.syncElapsed()
        while (audio.phase == AudioPhase.Playing) {
            delay(250)
            audio.syncElapsed()
        }
    }
    val seekTo = { x: Float, width: Int -> audio.seek(audio.duration * (x / width).coerceIn(0f, 1f)) }
    Box(modifier.height(4.dp), contentAlignment = Alignment.CenterStart) {
        Box(Modifier.fillMaxWidth().requiredHeight(48.dp)
            .pointerInput(audio) { detectTapGestures { seekTo(it.x, size.width) } }
            .pointerInput(audio) { detectHorizontalDragGestures { change, _ -> seekTo(change.position.x, size.width) } },
            contentAlignment = Alignment.CenterStart) {
            Box(Modifier.fillMaxWidth().height(4.dp).clip(Capsule()).background(Palette.tertiary)) {
                Box(Modifier.fillMaxWidth((audio.elapsed / audio.duration).toFloat().coerceIn(0f, 1f)).height(4.dp).clip(Capsule()).background(Palette.text))
            }
        }
    }
}

/** Seconds TalkBack's adjust moves a voice note's playhead, as VoiceOver's on iOS and the arrow keys on desktop. */
private const val SEEK_STEP = 5.0

/** TalkBack reads a voice note's position and moves it five seconds (swipe up/down or the volume keys) once its length is known. */
internal fun SemanticsPropertyReceiver.audioSeek(audio: SharedAudio?) {
    if (audio == null || audio.duration <= 0.0) return
    val elapsed = audio.elapsed.toFloat()
    progressBarRangeInfo = ProgressBarRangeInfo(elapsed, 0f..audio.duration.toFloat())
    setProgress { audio.seek(seekStepTarget(audio.elapsed, it.compareTo(elapsed))); true }
}

/** Where an accessibility adjust toward `direction` (its sign; 0 at either end) moves a playhead at `elapsed`: one [SEEK_STEP]
 *  that way, whatever value the service asked for, so every note moves alike whatever its length. */
internal fun seekStepTarget(elapsed: Double, direction: Int): Double = elapsed + SEEK_STEP * direction.sign

/** Seconds as `m:ss` (`h:mm:ss` past an hour). */
internal fun clock(seconds: Double): String {
    val total = seconds.roundToInt()
    return if (total >= 3600) "%d:%02d:%02d".format(total / 3600, total / 60 % 60, total % 60) else "%d:%02d".format(total / 60, total % 60)
}

/** A shared file's caption: a player failure, then how its download goes, then the sound's progress, then its size. */
internal fun detail(file: SharedFile, download: FileDownloadState?, audio: SharedAudio? = null, tr: (String) -> String = { it }): String {
    if (audio?.phase == AudioPhase.Failed) return tr("Couldn't play. Tap to retry.")
    val size = formatBytes(file.size)
    val progress = audio?.takeIf { it.duration > 0.0 }?.let { "${clock(it.elapsed)} / ${clock(it.duration)}" }
    return when (download) {
        is FileDownloadState.Downloading -> tr("{received} of {size}").replace("{received}", formatBytes(download.received)).replace("{size}", size)
        is FileDownloadState.Failed -> "${download.message} ${tr("Tap to retry.")}"
        FileDownloadState.Saved -> progress?.let { "$it · ${tr("Downloaded")}" } ?: "$size · ${tr("Downloaded")}"
        null -> progress ?: size
    }
}
