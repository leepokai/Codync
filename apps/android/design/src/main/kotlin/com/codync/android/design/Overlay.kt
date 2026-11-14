package com.codync.android.design

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.currentCompositionLocalContext
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier

/**
 * Sheets, menus and dialogs draw in the app's own window, above everything, so their glass can
 * refract what's under them (a system popup window can't see the app's layers).
 */
class OverlayHost internal constructor() {
    internal val entries = mutableStateListOf<Entry>()

    internal class Entry(val content: @Composable () -> Unit)
}

private val LocalOverlayHost = staticCompositionLocalOf<OverlayHost?> { null }

/** The app root: content, then every open overlay in the order it opened. */
@Composable fun OverlayRoot(content: @Composable () -> Unit) {
    val host = remember { OverlayHost() }
    CompositionLocalProvider(LocalOverlayHost provides host) {
        Box(Modifier.fillMaxSize()) {
            content()
            for (entry in host.entries) key(entry) { entry.content() }
        }
    }
}

/** Draws `content` on top of the app while this stays composed, keeping the caller's locals. */
@Composable fun Overlay(content: @Composable () -> Unit) {
    val host = requireNotNull(LocalOverlayHost.current) { "Overlay needs an OverlayRoot" }
    val locals by rememberUpdatedState(currentCompositionLocalContext)
    val latest by rememberUpdatedState(content)
    val entry = remember { OverlayHost.Entry { CompositionLocalProvider(locals) { latest() } } }
    DisposableEffect(entry) {
        host.entries.add(entry)
        onDispose { host.entries.remove(entry) }
    }
}
