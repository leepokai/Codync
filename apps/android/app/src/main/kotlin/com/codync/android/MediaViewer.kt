package com.codync.android

import android.net.Uri
import android.widget.MediaController
import android.widget.VideoView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.codync.android.design.*
import java.io.File
import kotlinx.coroutines.CancellationException

/**
 * Pictures and clips full size, paged sideways while `items` is set (iOS image viewer). The header
 * names the page and counts it; [trailing] adds the page's own actions.
 */
@Composable internal fun <T : Any> MediaViewerSheet(items: List<T>?, start: Int, name: (T) -> String, dismiss: () -> Unit,
    trailing: (@Composable (T) -> Unit)? = null, content: @Composable (T, Modifier, Boolean) -> Unit) {
    if (items == null) return
    CodyncSheet(true, dismiss) {
        val pager = rememberPagerState(initialPage = start.coerceIn(0, items.lastIndex)) { items.size }
        Column(Modifier.fillMaxSize().background(Palette.background)) {
            ModalHeader(name(items[pager.currentPage])) {
                Text("${pager.currentPage + 1} of ${items.size}", Modifier.padding(end = 8.dp), style = Type.caption, color = Palette.tertiary)
                trailing?.invoke(items[pager.currentPage])
            }
            HorizontalPager(pager, Modifier.fillMaxSize(), contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 12.dp),
                pageSpacing = 12.dp) { page ->
                Box(Modifier.fillMaxSize().padding(vertical = 12.dp), contentAlignment = Alignment.Center) {
                    content(items[page], Modifier.fillMaxSize(), page == pager.currentPage)
                }
            }
        }
    }
}

/** A clip played by the system player while its page is the one showing. */
@Composable internal fun VideoPlayer(load: suspend () -> File, active: Boolean, modifier: Modifier) {
    var file by remember { mutableStateOf<File?>(null) }
    var view by remember { mutableStateOf<VideoView?>(null) }
    LaunchedEffect(Unit) {
        try { file = load() } catch (cancelled: CancellationException) { throw cancelled } catch (_: Exception) { /* A still frame stays. */ }
    }
    val ready = file
    if (ready == null) {
        Box(modifier.background(Color.Black).semantics { contentDescription = "Loading video" }, contentAlignment = Alignment.Center) { Spinner() }
        return
    }
    AndroidView(factory = { context ->
        VideoView(context).apply {
            MediaController(context).also { controller -> controller.setAnchorView(this); setMediaController(controller) }
            setVideoURI(Uri.fromFile(ready))
            setOnPreparedListener { if (active) start() }
            view = this
        }
    }, update = { player ->
        if (active) { if (!player.isPlaying) player.start() } else if (player.isPlaying) player.pause()
    }, modifier = modifier.background(Color.Black))
    DisposableEffect(Unit) { onDispose { view?.stopPlayback() } }
}
