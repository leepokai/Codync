package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.NewMessageCounter
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

/** A conversation's scroll, shared by its list and the Jump to latest button floating in the screen's chrome. */
@Stable internal class ConversationScroll(val list: LazyListState) {
    /** On the newest message, following new ones; false once the reader scrolls away. */
    val following = mutableStateOf(true)
    internal var hasMessages by mutableStateOf(false)
    private val counter = NewMessageCounter()
    /** How many messages arrived since the reader left the bottom; shown on Jump to latest (idea 31). */
    val newMessages = mutableStateOf(0)
    internal fun feed(ids: List<String>, atBottom: Boolean) { newMessages.value = counter.update(ids, atBottom) }
}

@Composable internal fun rememberConversationScroll(): ConversationScroll {
    val list = rememberLazyListState()
    return remember(list) { ConversationScroll(list) }
}

/**
 * A conversation's scrolling list (iOS `ConversationList`): it opens at the newest message and follows
 * it as messages arrive (`last`, `rev`, `working`) until the reader scrolls away; then `JumpToLatest`,
 * in the screen's chrome, leads back down. With `refresh`, a pull past the top runs it, its indicator at `indicatorTop`.
 */
@Composable internal fun ConversationList(scroll: ConversationScroll, ids: List<String>, last: String?, rev: Long?, working: Boolean, padding: PaddingValues,
    refresh: (suspend () -> Unit)? = null, indicatorTop: Dp = 0.dp, content: LazyListScope.() -> Unit) {
    val list = scroll.list
    FollowNewest(scroll, ids, last, rev, working, padding.calculateBottomPadding())
    SideEffect { scroll.hasMessages = last != null }
    if (refresh == null) LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = padding, content = content)
    else PullToRefresh(refresh, Modifier.fillMaxSize(), indicatorTop) {
        LazyColumn(Modifier.fillMaxSize(), state = list, contentPadding = padding, content = content)
    }
}

/** Stays on the newest message until the reader scrolls away; scrolling back down follows again. */
@Composable private fun FollowNewest(scroll: ConversationScroll, ids: List<String>, last: String?, rev: Long?, working: Boolean, bottom: Dp) {
    val list = scroll.list
    val following = scroll.following
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
        // The count can arrive mid-layout (the list is measured inside another layout): scroll on the next frame.
        withFrameNanos {}
        list.scrollToEnd(animated = false)
    }
    // The working steps can grow without a new chat entry. Follow their measured size,
    // not its offset (scrolling itself changes offsets), and always yield to the reader.
    LaunchedEffect(list) {
        snapshotFlow {
            val layout = list.layoutInfo
            layout.visibleItemsInfo.lastOrNull()?.takeIf { it.index == layout.totalItemsCount - 1 }
                ?.let { Triple(it.key, it.size, layout.viewportEndOffset - layout.viewportStartOffset) }
        }.collect { size ->
            if (size != null && following.value && !list.isScrollInProgress) list.scrollToEnd(animated = false)
        }
    }
    // A new message, or the box growing (keyboard, more lines): stay on the newest.
    LaunchedEffect(last, rev, working, bottom) {
        if (following.value && list.layoutInfo.totalItemsCount > 0) list.scrollToEnd(animated = true)
    }
    // Feed the "N new" counter on the message ids and whether the reader is on the newest (idea 31).
    LaunchedEffect(ids, following.value) { scroll.feed(ids, following.value) }
}

private suspend fun LazyListState.scrollToEnd(animated: Boolean) {
    val count = layoutInfo.totalItemsCount
    if (count == 0) return
    if (animated) animateScrollToItem(count - 1)
    scrollToItem(count - 1, Int.MAX_VALUE)
}

/**
 * Scrolled away from the newest messages: a round frosted button back down to them. Chrome of the
 * screen's `GlassStage`, so its frost blurs the messages scrolling under it.
 */
@Composable internal fun JumpToLatest(scroll: ConversationScroll, modifier: Modifier) {
    val scope = rememberCoroutineScope()
    AnimatedVisibility(!scroll.following.value && scroll.hasMessages, modifier, enter = fadeIn(Motion.layout()) + scaleIn(Motion.layout(), initialScale = 0.8f),
        exit = fadeOut(Motion.layout()) + scaleOut(Motion.layout(), targetScale = 0.8f)) {
        // iOS `frosted(in: Circle())`: grey at 82% over a blur of what scrolls beneath, lifted by a soft shadow.
        val frosted = Color(if (LocalDark.current) 0xFF232323 else 0xFFF4F4F4).copy(alpha = 0.82f)
        val count = scroll.newMessages.value
        Box(Modifier.shadow(8.dp, Capsule()).clip(Capsule()).frost(20.dp, frosted)) {
            Row(Modifier.height(36.dp).padding(horizontal = if (count > 0) 6.dp else 0.dp), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (count > 0) Text("$count new", style = Type.chatFootnote.semibold, color = Palette.accent, maxLines = 1)
                IconButton("Jump to latest", "arrow.down") {
                    scroll.following.value = true
                    scope.launch { scroll.list.scrollToEnd(animated = true) }
                }
            }
        }
    }
}
