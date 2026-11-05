package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.expandVertically
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** While the bot works (anywhere: a room, a routine), the user's waiting messages show above the composer instead of in
 *  the chat (iOS `queue(working:)`), oldest first. A bot that isn't working starts them at once. */
internal fun queuedMessages(chat: List<Entry>, working: Boolean): List<Entry> =
    if (working) chat.filter(Entry::isQueued).sortedBy(Entry::seq) else emptyList()

/** The first host version that takes a queued message now (`steer`) or back (`unqueue`). */
private const val STEER_HOST = "2.39.0"

/** An older host has neither, so its queued messages only show; before its `hello` (offline) the actions stay hidden too. */
internal val AppState.supportsQueueActions: Boolean
    get() = hello?.text("version")?.let { !AppVersion.isBelow(it, STEER_HOST) } == true

/**
 * The user's messages waiting behind the bot's work, as iOS `QueuedMessages`: each one line, sent now into the running
 * turn ([steer]) or taken back ([unqueue]).
 */
@Composable internal fun QueuedMessages(entries: List<Entry>, state: AppState, store: AppStore, modifier: Modifier = Modifier) {
    // The card keeps its last rows while it folds away.
    var shown by remember { mutableStateOf(entries) }
    if (entries.isNotEmpty() && entries != shown) shown = entries
    val actions = state.supportsQueueActions
    val scroll = rememberScrollState()
    val newest = entries.lastOrNull()
    // It stays where it is, the oldest (next to go) in view; a message sent from here shows that it arrived.
    LaunchedEffect(newest?.id) {
        if (newest?.clientNonce != null && newest.clientNonce == store.lastSentNonce) {
            withFrameNanos {} // Its row is laid out by then, so the scroll takes it in.
            scroll.animateScrollTo(scroll.maxValue)
        }
    }
    AnimatedVisibility(entries.isNotEmpty(), modifier, enter = fadeIn(Motion.fade()) + expandVertically(Motion.layoutSize),
        exit = fadeOut(Motion.fade()) + shrinkVertically(Motion.layoutSize)) {
        // A long queue scrolls, so every row stays in reach and the chat keeps its room.
        Column(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Palette.surface).animateContentSize(Motion.layoutSize)
            .heightIn(max = 190.dp).verticalScroll(scroll).padding(vertical = 2.dp)) {
            for (entry in shown) key(entry.id) {
                QueuedRow(entry, actions, steering = "steer:" + entry.id in state.busy, steer = { store.steer(entry) }, remove = { store.unqueue(entry) })
            }
        }
    }
}

@Composable private fun QueuedRow(entry: Entry, actions: Boolean, steering: Boolean, steer: () -> Unit, remove: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 14.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(2.dp)) {
        Text(messagePreview(entry.replyQuote.text), Modifier.weight(1f).padding(vertical = if (actions) 0.dp else 9.dp),
            style = Type.subheadline, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (actions) {
            IconButton("Send now", "arrow.turn.down.right", enabled = !steering, action = steer)
            IconButton("Remove", "trash", enabled = !steering, action = remove)
        }
    }
}
