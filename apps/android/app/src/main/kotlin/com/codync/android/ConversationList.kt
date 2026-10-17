package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/**
 * A conversation's scrolling list (iOS `ConversationList`): it opens at the newest message and follows
 * it as messages arrive (`last`, `rev`, `working`) until the reader scrolls away; then a round button
 * leads back down. With `refresh`, a pull past the top runs it, its indicator at `indicatorTop`.
 */
@Composable internal fun ConversationList(last: String?, rev: Long?, working: Boolean, padding: PaddingValues,
    refresh: (suspend () -> Unit)? = null, indicatorTop: Dp = 0.dp, content: LazyListScope.() -> Unit) {
    val list = rememberLazyListState()
    val bottom = padding.calculateBottomPadding()
    val following = rememberFollowing(list, last, rev, working, bottom)
    val scope = rememberCoroutineScope()
    Box(Modifier.fillMaxSize()) {
        if (refresh == null) LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = padding, content = content)
        else PullToRefresh(refresh, Modifier.fillMaxSize(), indicatorTop) {
            LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = padding, content = content)
        }
        // 10 above the composer, as on iOS: the list already keeps 8 under its last row.
        JumpToLatest(!following.value && last != null, Modifier.align(Alignment.BottomCenter).padding(bottom = bottom + 2.dp)) {
            following.value = true
            scope.launch { list.scrollToEnd(animated = true) }
        }
    }
}

/** Stays on the newest message until the reader scrolls away; scrolling back down follows again. */
@Composable private fun rememberFollowing(list: LazyListState, last: String?, rev: Long?, working: Boolean, bottom: Dp): MutableState<Boolean> {
    val following = remember { mutableStateOf(true) }
    LaunchedEffect(list) {
        // Only the reader's own scroll decides: new content arriving (even while Codync is in the background) keeps the follow.
        var touched = false
        launch { list.interactionSource.interactions.collect { if (it is DragInteraction.Start) touched = true } }
        snapshotFlow { list.isScrollInProgress }.collect { scrolling ->
            if (!scrolling && touched) { touched = false; following.value = !list.canScrollForward }
        }
    }
    LaunchedEffect(Unit) {
        snapshotFlow { list.layoutInfo.totalItemsCount }.first { it > 0 }
        list.scrollToEnd(animated = false)
    }
    // A new message, or the box growing (keyboard, more lines): stay on the newest.
    LaunchedEffect(last, rev, working, bottom) {
        if (following.value && list.layoutInfo.totalItemsCount > 0) list.scrollToEnd(animated = true)
    }
    return following
}

private suspend fun LazyListState.scrollToEnd(animated: Boolean) {
    val count = layoutInfo.totalItemsCount
    if (count == 0) return
    if (animated) animateScrollToItem(count - 1)
    scrollToItem(count - 1, Int.MAX_VALUE)
}

/** Scrolled away from the newest messages: a round button back down to them. */
@Composable private fun JumpToLatest(visible: Boolean, modifier: Modifier, action: () -> Unit) {
    AnimatedVisibility(visible, modifier, enter = fadeIn(Motion.layout()) + scaleIn(Motion.layout(), initialScale = 0.8f),
        exit = fadeOut(Motion.layout()) + scaleOut(Motion.layout(), targetScale = 0.8f)) {
        // iOS `frosted(in: Circle())`: a near-opaque grey disc with a soft shadow.
        val frosted = Color(if (LocalDark.current) 0xFF232323 else 0xFFF4F4F4).copy(alpha = 0.82f)
        Box(Modifier.shadow(6.dp, CircleShape, ambientColor = Color.Black.copy(alpha = 0.12f),
            spotColor = Color.Black.copy(alpha = 0.12f)).background(frosted, CircleShape)) {
            IconButton("Jump to latest", "arrow.down", action = action)
        }
    }
}
