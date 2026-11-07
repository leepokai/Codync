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
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.SemanticsPropertyReceiver
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.onClick
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlin.math.roundToInt
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** A file a bot shared, as iOS `FileDownloadCard`: an image shows itself (tap for full size), a voice note plays, then its
 *  name and size, and a button to download (or cancel). One accessibility element: `<name>, <caption>` plus the actions. */
@Composable internal fun FileDownloadCard(entry: Entry, file: SharedFile, state: AppState, store: AppStore) {
    var viewing by remember { mutableStateOf(false) }
    val player = if (file.showsAsAudio) rememberSharedAudio(entry, file, store) else null
    val scope = rememberCoroutineScope()
    val translate = translator()
    val download = state.fileDownloads[file.id]
    val busy = download is FileDownloadState.Downloading
    val caption = detail(file, download, player, translate)
    val play = { player?.let { scope.launch { it.toggle() } }; Unit }
    val save = { if (busy) store.cancelFile(file.id) else store.saveFile(entry.id, entry.botId, file) }
    val view = if (file.showsAsImage) ({ viewing = true }) else null
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.bubbleAgent)
        .clearAndSetSemantics {
            contentDescription = "${file.name}, $caption"
            audioSeek(player)
            customActions = listOfNotNull(
                player?.let { audio -> CustomAccessibilityAction(translate(if (audio.phase == AudioPhase.Playing) "Pause" else "Play")) { play(); true } },
                CustomAccessibilityAction(translate(if (busy) "Cancel download" else "Download")) { save(); true },
                view?.let { open -> CustomAccessibilityAction(translate("View")) { open(); true } })
            onClick(label = when {
                view != null -> translate("View")
                player != null -> translate(if (player.phase == AudioPhase.Playing) "Pause" else "Play")
                else -> translate(if (busy) "Cancel download" else "Download")
            }) { (view ?: player?.let { play } ?: save)(); true }
        }.padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (file.showsAsImage) SharedImage(file, entry.id, 720, state, store, Modifier.sizeIn(maxHeight = 280.dp)
            .clip(RoundedRectangle(10.dp)).plainButton { viewing = true })
        FileRow(file, download, player, caption, save, play)
    }

    SharedImageSheet(file.takeIf { viewing }, entry.id, state, store) { viewing = false }
}

@Composable private fun FileRow(file: SharedFile, download: FileDownloadState?, audio: SharedAudio?, caption: String,
    save: () -> Unit, play: () -> Unit) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        if (audio != null) SharedAudioButton(audio, play) else Symbol(AttachmentIcon.symbol(file.name), size = 24.dp, tint = Palette.secondary)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(file.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(caption, style = Type.caption2, color = if (audio?.phase == AudioPhase.Failed) Palette.danger else Palette.secondary)
            if (audio != null && audio.duration > 0.0) AudioProgress(audio)
        }
        Box(Modifier.size(48.dp).plainButton(action = save), contentAlignment = Alignment.Center) {
            Symbol(if (download is FileDownloadState.Downloading) "xmark" else "arrow.down.to.line", size = 20.dp, tint = Palette.text)
        }
    }
}

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

/** TalkBack reads a voice note's position and moves it (swipe up/down or the volume keys) once its length is known. */
internal fun SemanticsPropertyReceiver.audioSeek(audio: SharedAudio?) {
    if (audio == null || audio.duration <= 0.0) return
    progressBarRangeInfo = ProgressBarRangeInfo(audio.elapsed.toFloat(), 0f..audio.duration.toFloat())
    setProgress { audio.seek(it.toDouble()); true }
}

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
