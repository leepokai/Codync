package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.core.SharedFile
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** A file a bot shared, as iOS `FileDownloadCard`: its name and size, and a button to download (or cancel). */
@Composable internal fun FileDownloadCard(entry: Entry, file: SharedFile, state: AppState, store: AppStore) {
    val download = state.fileDownloads[file.id]
    val busy = download is FileDownloadState.Downloading
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.bubbleAgent).padding(14.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Symbol(AttachmentIcon.symbol(file.name), size = 24.dp, tint = Palette.secondary)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(file.name, style = Type.subheadline.semibold, color = Palette.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(detail(file, download), style = Type.caption2, color = Palette.secondary)
        }
        Box(Modifier.size(36.dp).plainButton { if (busy) store.cancelFile(file.id) else store.saveFile(entry.id, entry.botId, file) }
            .semantics { contentDescription = (if (busy) "Cancel download " else "Download ") + file.name }, contentAlignment = Alignment.Center) {
            Symbol(if (busy) "xmark" else "arrow.down.to.line", size = 20.dp, tint = Palette.text)
        }
    }
}

/** A shared file's size, or how its download goes. */
internal fun detail(file: SharedFile, download: FileDownloadState?): String {
    val size = formatBytes(file.size)
    return when (download) {
        is FileDownloadState.Downloading -> "${formatBytes(download.received)} of $size"
        is FileDownloadState.Failed -> "${download.message} Tap to retry."
        FileDownloadState.Saved -> "$size · Downloaded"
        null -> size
    }
}
