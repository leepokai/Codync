package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** One picture or clip in a message, as the mosaic lists it. */
internal data class MediaItem(val key: String, val name: String, val video: Boolean)

/**
 * The media of one message (iOS image stack): alone at its own ratio, two side by side, three or
 * more as a grid whose last tile says how many more. A tap opens the viewer at that tile.
 */
@Composable internal fun MediaMosaic(items: List<MediaItem>, open: (Int) -> Unit, tile: @Composable (MediaItem, Modifier) -> Unit) {
    if (items.isEmpty()) return
    if (items.size == 1) {
        val item = items.first()
        Box(Modifier.sizeIn(maxWidth = 220.dp, maxHeight = 280.dp).clip(RoundedRectangle(16.dp))
            .semantics { contentDescription = item.name }.plainButton { open(0) }) {
            tile(item, Modifier.fillMaxSize())
        }
        return
    }
    val shown = items.take(4)
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        for (row in shown.chunked(2)) {
            Row(horizontalArrangement = Arrangement.spacedBy(2.dp)) {
                for (item in row) {
                    val index = items.indexOf(item)
                    Box(Modifier.size(108.dp).clip(RoundedRectangle(12.dp)).semantics { contentDescription = item.name }
                        .plainButton { open(index) }) {
                        tile(item, Modifier.fillMaxSize())
                        val more = items.size - shown.size
                        if (more > 0 && index == shown.lastIndex) {
                            Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = .45f)), contentAlignment = Alignment.Center) {
                                Text("+$more", style = Type.title3.semibold, color = Color.White)
                            }
                        }
                    }
                }
                if (row.size == 1) Box(Modifier.size(108.dp))
            }
        }
    }
}

/** A clip's tile before it plays: dark, with its name and a play glyph. */
@Composable internal fun VideoTile(name: String, modifier: Modifier = Modifier) {
    Box(modifier.background(Color.Black.copy(alpha = .78f)), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp),
            modifier = Modifier.padding(8.dp)) {
            Symbol("play.fill", size = 22.dp, tint = Color.White)
            Text(name, style = Type.caption2, color = Color.White.copy(alpha = .85f), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** A ratio box that keeps media from jumping once it loads. */
internal fun Modifier.mediaRatio(wide: Boolean): Modifier = aspectRatio(if (wide) 16f / 9f else 4f / 3f)
